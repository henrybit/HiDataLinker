import { Annotation, END, START, StateGraph } from '@langchain/langgraph';
import { z, type ZodType } from 'zod';
import { completeLlm, invokeTool } from '../host.js';
import { normalizeLocale, type AgentLocale } from '../data-analyst/prompts.js';
import {
	insightsStatus,
	profileResponseSchema,
	profileSystemPrompt,
	profileUserPrompt,
	recommendResponseSchema,
	recommendSystemPrompt,
	recommendUserPrompt,
	type ConnectionProfileResult
} from './prompts.js';

const MAX_INSIGHTS = 8;
const MAX_INSPECT_TABLES = 16;
const MAX_INSPECT_PER_CONN = 4;
const MAX_COLUMNS_PER_TABLE = 40;

export type InsightItem = {
	id: string;
	title: string;
	body: string;
	severity: 'high' | 'medium' | 'low';
	category: 'business' | 'quality' | 'performance' | 'governance' | 'integration';
	connectionIds: string[];
	tables: string[];
	tags: string[];
	prompt: string;
};

export type InsightsReport = {
	generatedAt: string;
	locale: AgentLocale;
	connections: { id: string; domain: string; summary: string }[];
	items: InsightItem[];
};

export type InsightsEvent =
	| { type: 'status'; phase: string; message: string }
	| { type: 'profiles'; connections: { id: string; domain: string; summary: string }[] }
	| { type: 'error'; message: string };

export type InsightsLlmConfig = {
	provider: 'openai' | 'anthropic';
	model: string;
	baseUrl: string;
};

const InsightsAnnotation = Annotation.Root({
	schemaDigest: Annotation<string>,
	knownConnectionIds: Annotation<string[]>,
	profiles: Annotation<ConnectionProfileResult[]>,
	columnNotes: Annotation<string[]>,
	items: Annotation<InsightItem[]>
});

type InsightsState = typeof InsightsAnnotation.State;

function toJsonSchema(schema: ZodType): Record<string, unknown> {
	const json = { ...(z.toJSONSchema(schema) as Record<string, unknown>) };
	delete json.$schema;
	return json;
}

async function hostComplete<T>(
	runId: string,
	llm: InsightsLlmConfig,
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
	if (!reply.ok) throw new Error(reply.error?.trim() || 'llm complete failed');
	return schema.parse(reply.result);
}

/** Connection ids as written by the host digest: `- name (engine) id=<id> cache=…`. */
export function parseDigestConnectionIds(digest: string): string[] {
	const ids: string[] = [];
	for (const match of digest.matchAll(/\bid=([^\s]+)/g)) {
		if (!ids.includes(match[1])) ids.push(match[1]);
	}
	return ids;
}

function formatColumns(raw: unknown): string {
	if (!Array.isArray(raw)) return '(no columns)';
	return raw
		.slice(0, MAX_COLUMNS_PER_TABLE)
		.map((item) => {
			const col = item as {
				name?: string;
				columnType?: string;
				dataType?: string;
				nullable?: boolean;
				key?: string;
				comment?: string;
			};
			const parts = [col.name ?? '?', col.columnType || col.dataType || ''];
			if (col.key) parts.push(`[${col.key}]`);
			if (col.nullable === false) parts.push('not null');
			if (col.comment) parts.push(`// ${col.comment.slice(0, 40)}`);
			return parts.filter(Boolean).join(' ');
		})
		.join('; ');
}

function slug(text: string): string {
	return text
		.toLowerCase()
		.replace(/[^\p{L}\p{N}]+/gu, '-')
		.replace(/^-+|-+$/g, '')
		.slice(0, 48);
}

export async function runInsights(input: {
	runId: string;
	locale?: string;
	schemaDigest?: string;
	llm: InsightsLlmConfig;
	signal?: AbortSignal;
	onEvent?: (event: InsightsEvent) => void;
}): Promise<{ report?: InsightsReport; error?: string }> {
	const locale = normalizeLocale(input.locale);
	const throwIfAborted = () => {
		if (input.signal?.aborted) throw new Error('cancelled');
	};
	const status = (phase: Parameters<typeof insightsStatus>[1]) =>
		input.onEvent?.({ type: 'status', phase, message: insightsStatus(locale, phase) });

	const collect = async (_state: InsightsState) => {
		throwIfAborted();
		status('collect');
		const schemaDigest = input.schemaDigest?.trim() || '(no connections)';
		return { schemaDigest, knownConnectionIds: parseDigestConnectionIds(schemaDigest) };
	};

	const profile = async (state: InsightsState) => {
		throwIfAborted();
		status('profile');
		const response = await hostComplete(
			input.runId,
			input.llm,
			profileResponseSchema,
			profileSystemPrompt(locale),
			profileUserPrompt(state.schemaDigest)
		);
		const known = new Set(state.knownConnectionIds);
		const profiles = response.connections.filter(
			(item) => known.size === 0 || known.has(item.connectionId)
		);
		input.onEvent?.({
			type: 'profiles',
			connections: profiles.map((item) => ({
				id: item.connectionId,
				domain: item.domain,
				summary: item.summary
			}))
		});
		return { profiles };
	};

	const inspect = async (state: InsightsState) => {
		throwIfAborted();
		status('inspect');
		const notes: string[] = [];
		let budget = MAX_INSPECT_TABLES;
		for (const conn of state.profiles) {
			for (const key of conn.keyTables.slice(0, MAX_INSPECT_PER_CONN)) {
				if (budget <= 0) break;
				throwIfAborted();
				budget -= 1;
				const result = await invokeTool(input.runId, 'get_columns', {
					connectionId: conn.connectionId,
					schema: key.schema,
					table: key.table
				});
				const label = `${conn.connectionId} ${key.schema}.${key.table}`;
				notes.push(
					result.ok
						? `- ${label}: ${formatColumns(result.result)}`
						: `- ${label}: (columns unavailable: ${result.error ?? 'error'})`
				);
			}
		}
		return { columnNotes: notes };
	};

	const recommend = async (state: InsightsState) => {
		throwIfAborted();
		status('recommend');
		const response = await hostComplete(
			input.runId,
			input.llm,
			recommendResponseSchema,
			recommendSystemPrompt(locale),
			recommendUserPrompt({
				schemaDigest: state.schemaDigest,
				profiles: state.profiles,
				columnNotes: state.columnNotes,
				maxInsights: MAX_INSIGHTS
			})
		);
		const items: InsightItem[] = response.insights.map((item, index) => ({
			id: `${index + 1}-${slug(item.title) || 'insight'}`,
			title: item.title.trim(),
			body: item.body.trim(),
			severity: item.severity,
			category: item.category,
			connectionIds: item.connectionIds,
			tables: item.tables,
			tags: item.tags.slice(0, 4),
			prompt: item.prompt.trim()
		}));
		return { items };
	};

	const finalize = async (state: InsightsState) => {
		status('finalize');
		const known = new Set(state.knownConnectionIds);
		const seen = new Set<string>();
		const rank = { high: 0, medium: 1, low: 2 } as const;
		const items = state.items
			.filter((item) => item.title && item.prompt)
			.map((item) => ({
				...item,
				connectionIds: item.connectionIds.filter((id) => known.size === 0 || known.has(id))
			}))
			.filter((item) => {
				const key = item.title.toLowerCase();
				if (seen.has(key)) return false;
				seen.add(key);
				return true;
			})
			.sort((a, b) => rank[a.severity] - rank[b.severity])
			.slice(0, MAX_INSIGHTS);
		return { items };
	};

	const afterCollect = (state: InsightsState) =>
		state.knownConnectionIds.length === 0 ? END : 'profile';

	const graph = new StateGraph(InsightsAnnotation)
		.addNode('collect', collect)
		.addNode('profile', profile)
		.addNode('inspect', inspect)
		.addNode('recommend', recommend)
		.addNode('finalize', finalize)
		.addEdge(START, 'collect')
		.addConditionalEdges('collect', afterCollect, ['profile', END])
		.addEdge('profile', 'inspect')
		.addEdge('inspect', 'recommend')
		.addEdge('recommend', 'finalize')
		.addEdge('finalize', END)
		.compile();

	try {
		const result = await graph.invoke({
			schemaDigest: '',
			knownConnectionIds: [],
			profiles: [],
			columnNotes: [],
			items: []
		});
		if (result.knownConnectionIds.length === 0) {
			return {
				error:
					locale === 'zh'
						? '暂无可分析的连接元数据，请先添加并连接数据源。'
						: 'No connection metadata available yet. Add and connect a data source first.'
			};
		}
		return {
			report: {
				generatedAt: new Date().toISOString(),
				locale,
				connections: result.profiles.map((item) => ({
					id: item.connectionId,
					domain: item.domain,
					summary: item.summary
				})),
				items: result.items
			}
		};
	} catch (caught) {
		const message = caught instanceof Error ? caught.message : String(caught);
		input.onEvent?.({ type: 'error', message });
		return { error: message };
	}
}
