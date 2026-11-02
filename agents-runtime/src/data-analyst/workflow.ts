import { Annotation, END, START, StateGraph } from '@langchain/langgraph';
import { z, type ZodType } from 'zod';
import { completeLlm, invokeTool, summarizeToolResult } from '../host.js';
import type { AnalystEvent, AnalystObservation, AnalystPlanStep } from './events.js';
import {
	actResponseSchema,
	actSystemPrompt,
	actUserPrompt,
	fallbackStepNote,
	normalizeLocale,
	planResponseSchema,
	planSystemPrompt,
	planUserPrompt,
	statusMessage,
	synthesizeResponseSchema,
	synthesizeSystemPrompt,
	synthesizeUserPrompt,
	type AgentLocale
} from './prompts.js';

const MAX_STEPS = 8;
const MAX_SCHEMAS_PER_CONN = 4;
const MAX_TABLES_PER_SCHEMA = 40;

const AnalystAnnotation = Annotation.Root({
	userQuery: Annotation<string>,
	systemMd: Annotation<string>,
	schemaDigest: Annotation<string>,
	plan: Annotation<AnalystPlanStep[]>({
		reducer: (_left, right) => right,
		default: () => []
	}),
	stepIndex: Annotation<number>,
	observations: Annotation<AnalystObservation[]>({
		reducer: (left, right) => left.concat(right),
		default: () => []
	}),
	finalMarkdown: Annotation<string>,
	error: Annotation<string>
});

type AnalystState = typeof AnalystAnnotation.State;

export type RunLlmConfig = {
	provider: 'openai' | 'anthropic';
	model: string;
	baseUrl: string;
};

function toJsonSchema(schema: ZodType): Record<string, unknown> {
	const json = { ...(z.toJSONSchema(schema) as Record<string, unknown>) };
	delete json.$schema;
	return json;
}

async function hostComplete<T>(
	runId: string,
	llm: RunLlmConfig,
	schema: ZodType<T>,
	system: string,
	human: string
): Promise<T> {
	const reply = await completeLlm({
		runId,
		provider: llm.provider,
		model: llm.model,
		baseUrl: llm.baseUrl,
		system,
		human,
		schema: toJsonSchema(schema)
	});
	if (!reply.ok) {
		throw new Error(reply.error?.trim() || 'llm complete failed');
	}
	return schema.parse(reply.result);
}

async function callSkillTool(runId: string, name: string, args: Record<string, unknown> = {}) {
	return invokeTool(runId, name, args);
}

async function buildSchemaDigestLive(
	runId: string,
	locale: AgentLocale,
	onEvent?: (event: AnalystEvent) => void
): Promise<string> {
	onEvent?.({ type: 'status', message: statusMessage(locale, 'gathering') });
	const listed = await callSkillTool(runId, 'list_connections');
	if (!listed.ok) {
		return `list_connections failed: ${listed.error ?? 'unknown'}`;
	}
	const connections = Array.isArray(listed.result) ? listed.result : [];
	const lines: string[] = [];
	for (const raw of connections.slice(0, 12)) {
		const conn = raw as {
			id?: string;
			name?: string;
			engine?: string;
			connected?: boolean;
		};
		if (!conn.id) continue;
		let connected = conn.connected === true;
		if (!connected) {
			const joined = await callSkillTool(runId, 'connect', { connectionId: conn.id });
			connected = joined.ok;
		}
		lines.push(
			`- ${conn.name ?? conn.id} (${conn.engine ?? '?'}) id=${conn.id} connected=${connected}`
		);
		if (!connected) continue;
		const dbs = await callSkillTool(runId, 'list_databases', { connectionId: conn.id });
		if (!dbs.ok || !Array.isArray(dbs.result)) {
			lines.push(`  databases: ${dbs.error ?? 'unavailable'}`);
			continue;
		}
		for (const dbRaw of dbs.result.slice(0, MAX_SCHEMAS_PER_CONN)) {
			const db = dbRaw as { name?: string };
			const schema = db.name?.trim();
			if (!schema) continue;
			const tables = await callSkillTool(runId, 'list_tables', {
				connectionId: conn.id,
				schema
			});
			const names = Array.isArray(tables.result)
				? tables.result
						.map((item) => (item as { name?: string }).name)
						.filter((name): name is string => !!name)
						.slice(0, MAX_TABLES_PER_SCHEMA)
				: [];
			lines.push(
				`  schema ${schema}: ${names.length ? names.join(', ') : tables.error ?? '(no tables)'}`
			);
		}
	}
	return lines.join('\n') || '(no connections)';
}

async function buildSchemaDigest(
	runId: string,
	locale: AgentLocale,
	cachedDigest: string | undefined,
	onEvent?: (event: AnalystEvent) => void
): Promise<string> {
	const cached = cachedDigest?.trim();
	if (cached) {
		onEvent?.({ type: 'status', message: statusMessage(locale, 'gathering') });
		return cached;
	}
	return buildSchemaDigestLive(runId, locale, onEvent);
}

export async function runDataAnalyst(input: {
	runId: string;
	userQuery: string;
	systemMd: string;
	locale?: AgentLocale | string;
	schemaDigest?: string;
	llm: RunLlmConfig;
	signal?: AbortSignal;
	onEvent?: (event: AnalystEvent) => void;
}): Promise<{ markdown: string; error?: string }> {
	const locale = normalizeLocale(input.locale);
	const throwIfAborted = () => {
		if (input.signal?.aborted) throw new Error('cancelled');
	};

	const gather = async (_state: AnalystState) => {
		throwIfAborted();
		const schemaDigest = await buildSchemaDigest(
			input.runId,
			locale,
			input.schemaDigest,
			input.onEvent
		);
		return { schemaDigest };
	};

	const makePlan = async (state: AnalystState) => {
		throwIfAborted();
		input.onEvent?.({ type: 'status', message: statusMessage(locale, 'planning') });
		const response = await hostComplete(
			input.runId,
			input.llm,
			planResponseSchema,
			planSystemPrompt(state.systemMd, locale),
			planUserPrompt(state.userQuery, state.schemaDigest, locale)
		);
		const steps = response.steps.slice(0, MAX_STEPS).map((step) => ({
			title: step.title.trim() || 'Step',
			goal: step.goal.trim() || step.title,
			connectionHint: step.connectionHint,
			tablesHint: step.tablesHint,
			toolHint: step.toolHint,
			sqlIntent: step.sqlIntent
		}));
		input.onEvent?.({ type: 'plan', steps });
		return { plan: steps, stepIndex: 0 };
	};

	const actStep = async (state: AnalystState) => {
		throwIfAborted();
		const step = state.plan[state.stepIndex];
		if (!step) return { stepIndex: state.stepIndex + 1 };
		input.onEvent?.({
			type: 'step-start',
			index: state.stepIndex,
			total: state.plan.length,
			title: step.title
		});
		const decision = await hostComplete(
			input.runId,
			input.llm,
			actResponseSchema,
			actSystemPrompt(state.systemMd, locale),
			actUserPrompt({
				query: state.userQuery,
				schemaDigest: state.schemaDigest,
				step,
				stepIndex: state.stepIndex,
				total: state.plan.length,
				observations: state.observations,
				locale
			})
		);

		const observations: AnalystObservation[] = [];
		if (decision.tool && decision.tool !== 'none') {
			const args = (decision.args ?? {}) as Record<string, unknown>;
			const started = performance.now();
			const result = await callSkillTool(input.runId, decision.tool, args);
			const durationMs = Math.round(performance.now() - started);
			const summary = summarizeToolResult(decision.tool, result);
			const observation: AnalystObservation = {
				stepIndex: state.stepIndex,
				tool: decision.tool,
				args,
				ok: result.ok,
				summary
			};
			observations.push(observation);
			input.onEvent?.({
				type: 'tool',
				index: state.stepIndex,
				tool: decision.tool,
				args,
				ok: result.ok,
				summary,
				durationMs
			});
		}

		const note = decision.note.trim() || fallbackStepNote(locale, decision.done);
		input.onEvent?.({ type: 'step-done', index: state.stepIndex, note });

		return {
			observations,
			stepIndex: state.stepIndex + 1
		};
	};

	const synthesize = async (state: AnalystState) => {
		throwIfAborted();
		input.onEvent?.({ type: 'status', message: statusMessage(locale, 'writing') });
		const response = await hostComplete(
			input.runId,
			input.llm,
			synthesizeResponseSchema,
			synthesizeSystemPrompt(state.systemMd, locale),
			synthesizeUserPrompt({
				query: state.userQuery,
				plan: state.plan,
				observations: state.observations,
				locale
			})
		);
		const markdown = response.markdown.trim();
		input.onEvent?.({ type: 'final', markdown });
		return { finalMarkdown: markdown };
	};

	const afterAct = (state: AnalystState) => {
		if (state.error) return 'synthesize';
		if (state.stepIndex >= state.plan.length || state.stepIndex >= MAX_STEPS) {
			return 'synthesize';
		}
		return 'actStep';
	};

	const graph = new StateGraph(AnalystAnnotation)
		.addNode('gather', gather)
		.addNode('makePlan', makePlan)
		.addNode('actStep', actStep)
		.addNode('synthesize', synthesize)
		.addEdge(START, 'gather')
		.addEdge('gather', 'makePlan')
		.addEdge('makePlan', 'actStep')
		.addConditionalEdges('actStep', afterAct, ['actStep', 'synthesize'])
		.addEdge('synthesize', END)
		.compile();

	try {
		const result = await graph.invoke(
			{
				userQuery: input.userQuery,
				systemMd: input.systemMd,
				schemaDigest: '',
				plan: [],
				stepIndex: 0,
				observations: [],
				finalMarkdown: '',
				error: ''
			},
			{ recursionLimit: 40 }
		);
		return {
			markdown: result.finalMarkdown || 'No analysis produced.',
			error: result.error || undefined
		};
	} catch (caught) {
		const message = caught instanceof Error ? caught.message : String(caught);
		input.onEvent?.({ type: 'error', message });
		return { markdown: '', error: message };
	}
}
