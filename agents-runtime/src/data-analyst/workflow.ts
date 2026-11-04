import { Annotation, END, START, StateGraph } from '@langchain/langgraph';
import { z, type ZodType } from 'zod';
import { completeLlm, invokeTool, summarizeToolResult } from '../host.js';
import type { AnalystEvent, AnalystObservation, AnalystPlanStep } from './events.js';
import {
	actResponseSchema,
	actSystemPrompt,
	actUserPrompt,
	fallbackStepNote,
	normalizeAuthMode,
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
import { resolveSkillTools, skillLabels } from './skills.js';

const MAX_STEPS = 8;
/** Max LLM/tool rounds inside a single plan step (ReAct inner loop). */
const MAX_ACTS_PER_STEP = 5;
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
	/** Tool/LLM iterations already spent on the current plan step. */
	actIters: Annotation<number>,
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
	onEvent?: (event: AnalystEvent) => void,
	connectionIds?: string[]
): Promise<string> {
	onEvent?.({ type: 'status', message: statusMessage(locale, 'gathering') });
	const listed = await callSkillTool(runId, 'list_connections');
	if (!listed.ok) {
		return `list_connections failed: ${listed.error ?? 'unknown'}`;
	}
	const allow = new Set((connectionIds ?? []).map((id) => id.trim()).filter(Boolean));
	const connections = (Array.isArray(listed.result) ? listed.result : []).filter((raw) => {
		if (allow.size === 0) return true;
		const conn = raw as { id?: string };
		return !!conn.id && allow.has(conn.id);
	});
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
	onEvent?: (event: AnalystEvent) => void,
	connectionIds?: string[]
): Promise<string> {
	const cached = cachedDigest?.trim();
	if (cached) {
		onEvent?.({ type: 'status', message: statusMessage(locale, 'gathering') });
		return cached;
	}
	return buildSchemaDigestLive(runId, locale, onEvent, connectionIds);
}

export async function runDataAnalyst(input: {
	runId: string;
	userQuery: string;
	systemMd: string;
	locale?: AgentLocale | string;
	schemaDigest?: string;
	connectionIds?: string[];
	skillIds?: string[];
	authMode?: string;
	conversationContext?: string;
	llm: RunLlmConfig;
	signal?: AbortSignal;
	onEvent?: (event: AnalystEvent) => void;
}): Promise<{ markdown: string; error?: string }> {
	const locale = normalizeLocale(input.locale);
	const authMode = normalizeAuthMode(input.authMode);
	const scopedIds = (input.connectionIds ?? []).map((id) => id.trim()).filter(Boolean);
	const skillIds = (input.skillIds ?? []).map((id) => id.trim()).filter(Boolean);
	const allowedTools = resolveSkillTools(skillIds);
	const skillLabelList = skillLabels(skillIds);
	const conversationContext = input.conversationContext?.trim() || undefined;
	const throwIfAborted = () => {
		if (input.signal?.aborted) throw new Error('cancelled');
	};

	const gather = async (_state: AnalystState) => {
		throwIfAborted();
		const schemaDigest = await buildSchemaDigest(
			input.runId,
			locale,
			input.schemaDigest,
			input.onEvent,
			scopedIds
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
			planUserPrompt(
				state.userQuery,
				state.schemaDigest,
				locale,
				scopedIds,
				conversationContext,
				skillLabelList,
				allowedTools.length > 0 ? allowedTools : undefined,
				authMode
			)
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
		return { plan: steps, stepIndex: 0, actIters: 0 };
	};

	const actStep = async (state: AnalystState) => {
		throwIfAborted();
		const step = state.plan[state.stepIndex];
		if (!step) return { stepIndex: state.stepIndex + 1, actIters: 0 };

		const actIters = state.actIters ?? 0;
		if (actIters === 0) {
			input.onEvent?.({
				type: 'step-start',
				index: state.stepIndex,
				total: state.plan.length,
				title: step.title
			});
		}

		const decision = await hostComplete(
			input.runId,
			input.llm,
			actResponseSchema,
			actSystemPrompt(state.systemMd, locale, authMode),
			actUserPrompt({
				query: state.userQuery,
				schemaDigest: state.schemaDigest,
				step,
				stepIndex: state.stepIndex,
				total: state.plan.length,
				observations: state.observations,
				locale,
				actIter: actIters,
				maxActIters: MAX_ACTS_PER_STEP,
				connectionIds: scopedIds,
				conversationContext,
				skillLabels: skillLabelList,
				allowedTools: allowedTools.length > 0 ? allowedTools : undefined,
				authMode
			})
		);

		const observations: AnalystObservation[] = [];
		const toolName = decision.tool && decision.tool !== 'none' ? decision.tool : null;
		if (toolName) {
			const args = (decision.args ?? {}) as Record<string, unknown>;
			const started = performance.now();
			const blocked =
				allowedTools.length > 0 && !allowedTools.includes(toolName)
					? {
							ok: false as const,
							error: `tool "${toolName}" is not allowed by selected skills (${skillLabelList.join(' ')})`
						}
					: null;
			const result = blocked ?? (await callSkillTool(input.runId, toolName, args));
			const durationMs = Math.round(performance.now() - started);
			const summary = summarizeToolResult(toolName, result);
			const observation: AnalystObservation = {
				stepIndex: state.stepIndex,
				tool: toolName,
				args,
				ok: result.ok,
				summary
			};
			observations.push(observation);
			input.onEvent?.({
				type: 'tool',
				index: state.stepIndex,
				tool: toolName,
				args,
				ok: result.ok,
				summary,
				durationMs
			});
		}

		const note = decision.note.trim() || fallbackStepNote(locale, decision.done);
		const nextActIters = actIters + 1;
		// Honor done=false: keep the same plan step so the model can call more tools.
		// Also re-enter when the model wrote a "next:" note without a tool (common stall).
		const wantsMore = !decision.done;
		const continueStep = wantsMore && nextActIters < MAX_ACTS_PER_STEP;

		if (continueStep) {
			input.onEvent?.({ type: 'status', message: note });
			return {
				observations,
				stepIndex: state.stepIndex,
				actIters: nextActIters
			};
		}

		input.onEvent?.({ type: 'step-done', index: state.stepIndex, note });
		return {
			observations,
			stepIndex: state.stepIndex + 1,
			actIters: 0
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
				locale,
				conversationContext
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
				actIters: 0,
				observations: [],
				finalMarkdown: '',
				error: ''
			},
			// gather + plan + synthesize + (MAX_STEPS * MAX_ACTS_PER_STEP) act rounds
			{ recursionLimit: 12 + MAX_STEPS * MAX_ACTS_PER_STEP }
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
