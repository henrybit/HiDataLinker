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
import type {
	AnalysisLocale,
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
	onProgress?: (phase: 'comments' | 'relations') => void;
}): Promise<RelationshipGraph> {
	const batchSize = Math.max(1, input.commentBatchSize ?? DEFAULT_COMMENT_BATCH);
	const aliases = objectAliases(input.catalog.objects);
	const pending = missingCommentItems(input.catalog, aliases);

	const inferComments = async (state: AnalysisState) => {
		input.onProgress?.('comments');
		const batch = pending.slice(state.commentCursor, state.commentCursor + batchSize);
		if (batch.length === 0) {
			return { commentCursor: state.commentCursor + batchSize };
		}
		const response = await input.caller.complete(
			commentResponseSchema,
			commentSystemPrompt(state.locale),
			commentUserPrompt(batch, state.documents)
		);
		return {
			comments: mapComments(response, batch),
			commentCursor: state.commentCursor + batchSize
		};
	};

	const inferRelations = async (state: AnalysisState) => {
		input.onProgress?.('relations');
		const commented = assembleGraph(state.catalog, state.comments, []).nodes;
		const catalog = { ...state.catalog, objects: commented };
		const known = assembleGraph(catalog, [], []).edges;
		const response = await input.caller.complete(
			relationResponseSchema,
			relationSystemPrompt(state.locale),
			relationUserPrompt(catalog, aliases, known, state.documents)
		);
		return { relations: mapRelations(response, aliases) };
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
	const result = await graph.invoke(
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
	return assembleGraph(input.catalog, result.comments ?? [], result.relations ?? []);
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
