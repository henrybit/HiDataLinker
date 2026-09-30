import { Annotation, END, START, StateGraph } from '@langchain/langgraph';
import type { ZodType } from 'zod';
import { assembleGraph } from './physical';
import {
	commentResponseSchema,
	commentSystemPrompt,
	commentUserPrompt,
	missingCommentItems,
	objectAliases,
	relationResponseSchema,
	relationSystemPrompt,
	relationUserPrompt,
	type CommentResponse,
	type RelationResponse
} from './prompts';
import { formatAnalysisError, type AnalysisLogEvent } from './log';
import type {
	AnalysisLocale,
	AnalysisWarning,
	InferredComment,
	InferredRelation,
	RelationshipGraph,
	SchemaCatalog
} from './types';

export interface StructuredCaller {
	complete<T>(schema: ZodType<T>, system: string, human: string): Promise<T>;
}

export const DEFAULT_COMMENT_BATCH = 24;

const AnalysisAnnotation = Annotation.Root({
	catalog: Annotation<SchemaCatalog>,
	documents: Annotation<string>,
	locale: Annotation<AnalysisLocale>,
	commentCursor: Annotation<number>,
	comments: Annotation<InferredComment[]>({
		reducer: (left, right) => left.concat(right),
		default: () => []
	}),
	relations: Annotation<InferredRelation[]>({
		reducer: (left, right) => left.concat(right),
		default: () => []
	})
});

type AnalysisState = typeof AnalysisAnnotation.State;

export async function inferRelationships(input: {
	catalog: SchemaCatalog;
	documents: string;
	locale: AnalysisLocale;
	caller: StructuredCaller;
	commentBatchSize?: number;
	onLog?: (event: AnalysisLogEvent) => void;
}): Promise<RelationshipGraph> {
	const batchSize = Math.max(1, input.commentBatchSize ?? DEFAULT_COMMENT_BATCH);
	const aliases = objectAliases(input.catalog.objects);
	const pending = missingCommentItems(input.catalog, aliases);
	const realObjects = input.catalog.objects.filter((object) => !object.external);
	const gaps: AnalysisWarning[] = [];
	if (realObjects.length === 0) input.onLog?.({ type: 'inference-skip' });
	else if (pending.length === 0) input.onLog?.({ type: 'comments-skip' });

	const inferComments = async (state: AnalysisState) => {
		const batch = pending.slice(state.commentCursor, state.commentCursor + batchSize);
		if (batch.length === 0) {
			return { commentCursor: state.commentCursor + batchSize };
		}
		const current = Math.floor(state.commentCursor / batchSize) + 1;
		const total = Math.max(1, Math.ceil(pending.length / batchSize));
		const human = commentUserPrompt(batch, state.documents);
		input.onLog?.({ type: 'comments-batch', current, total, count: batch.length });
		const started = performance.now();
		try {
			const response = await input.caller.complete(
				commentResponseSchema,
				commentSystemPrompt(state.locale),
				human
			);
			const comments = mapComments(response, batch);
			input.onLog?.({
				type: 'comments-batch-done',
				current,
				total,
				accepted: comments.length,
				durationMs: Math.round(performance.now() - started)
			});
			return {
				comments,
				commentCursor: state.commentCursor + batchSize
			};
		} catch (caught) {
			console.error('[analysis] comment batch failed', caught);
			input.onLog?.({
				type: 'comments-batch-failed',
				current,
				total,
				count: batch.length,
				durationMs: Math.round(performance.now() - started),
				ids: batch.slice(0, 8).map((item) => item.id),
				promptChars: human.length,
				detail: formatAnalysisError(caught)
			});
			if (!gaps.some((warning) => warning.code === 'comments')) {
				gaps.push({ code: 'comments', connectionName: '', schema: '' });
			}
			return {
				comments: [],
				commentCursor: state.commentCursor + batchSize
			};
		}
	};

	const inferRelations = async (state: AnalysisState) => {
		const commented = assembleGraph(state.catalog, state.comments, []).nodes;
		const catalog = { ...state.catalog, objects: commented };
		const known = assembleGraph(catalog, [], []).edges;
		input.onLog?.({
			type: 'relations-start',
			objects: commented.filter((object) => !object.external).length,
			known: known.length
		});
		const started = performance.now();
		try {
			const response = await input.caller.complete(
				relationResponseSchema,
				relationSystemPrompt(state.locale),
				relationUserPrompt(catalog, aliases, known, state.documents)
			);
			const relations = mapRelations(response, aliases);
			input.onLog?.({
				type: 'relations-done',
				returned: response.relations.length,
				kept: relations.length,
				durationMs: Math.round(performance.now() - started)
			});
			return { relations };
		} catch (caught) {
			const detail = formatAnalysisError(caught);
			console.error('[analysis] relationship inference failed', caught);
			input.onLog?.({
				type: 'relations-failed',
				durationMs: Math.round(performance.now() - started),
				detail
			});
			gaps.push({
				code: 'relations',
				connectionName: '',
				schema: '',
				detail: detail.split('\n')[0] ?? detail
			});
			return { relations: [] };
		}
	};

	const routeStart = (state: AnalysisState) => {
		const real = state.catalog.objects.some((object) => !object.external);
		if (!real) return END;
		return pending.length === 0 ? 'inferRelations' : 'inferComments';
	};

	const afterComments = (state: AnalysisState) =>
		state.commentCursor < pending.length ? 'inferComments' : 'inferRelations';

	const graph = new StateGraph(AnalysisAnnotation)
		.addNode('inferComments', inferComments)
		.addNode('inferRelations', inferRelations)
		.addConditionalEdges(START, routeStart, ['inferComments', 'inferRelations', END])
		.addConditionalEdges('inferComments', afterComments, ['inferComments', 'inferRelations'])
		.addEdge('inferRelations', END)
		.compile();

	const steps = Math.ceil(pending.length / batchSize) + 4;
	let result: AnalysisState;
	try {
		result = await graph.invoke(
			{
				catalog: input.catalog,
				documents: input.documents,
				locale: input.locale,
				commentCursor: 0,
				comments: [],
				relations: []
			},
			{ recursionLimit: Math.max(25, steps) }
		);
	} catch (caught) {
		const detail = formatAnalysisError(caught);
		input.onLog?.({ type: 'relations-failed', durationMs: 0, detail });
		if (!gaps.some((warning) => warning.code === 'relations')) {
			gaps.push({
				code: 'relations',
				connectionName: '',
				schema: '',
				detail: detail.split('\n')[0] ?? detail
			});
		}
		result = {
			catalog: input.catalog,
			documents: input.documents,
			locale: input.locale,
			commentCursor: pending.length,
			comments: [],
			relations: []
		};
	}
	const built = assembleGraph(input.catalog, result.comments ?? [], result.relations ?? []);
	return { ...built, warnings: [...built.warnings, ...gaps] };
}

function mapComments(
	response: CommentResponse,
	batch: ReturnType<typeof missingCommentItems>
): InferredComment[] {
	const byId = new Map(batch.map((item) => [item.id, item]));
	const seen = new Set<string>();
	const comments: InferredComment[] = [];
	for (const item of response.comments) {
		const target = byId.get(item.id);
		const text = item.comment.trim();
		if (!target || !text || seen.has(item.id)) continue;
		seen.add(item.id);
		comments.push({
			objectId: target.objectId,
			columnName: target.columnName,
			comment: text,
			confidence: item.confidence
		});
	}
	return comments;
}

function mapRelations(
	response: RelationResponse,
	aliases: Map<string, string>
): InferredRelation[] {
	const ids = new Map([...aliases.entries()].map(([id, alias]) => [alias, id]));
	const seen = new Set<string>();
	const relations: InferredRelation[] = [];
	for (const item of response.relations) {
		const fromId = ids.get(item.fromId);
		const toId = ids.get(item.toId);
		if (!fromId || !toId || fromId === toId) continue;
		const key = [
			fromId,
			toId,
			item.cardinality,
			item.fromColumns.join(','),
			item.toColumns.join(',')
		].join('|');
		if (seen.has(key)) continue;
		seen.add(key);
		relations.push({
			fromId,
			toId,
			fromColumns: item.fromColumns,
			toColumns: item.toColumns,
			strength: item.strength,
			cardinality: item.cardinality,
			reason: item.reason,
			confidence: item.confidence
		});
	}
	return relations.slice(0, 300);
}
