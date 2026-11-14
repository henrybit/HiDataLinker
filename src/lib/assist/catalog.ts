import { api } from '$lib/api/tauri';
import type { IndexInfo } from '$lib/api/types';
import { loadSchemaCatalog } from '$lib/analysis/collect';
import { asRelationshipGraph } from '$lib/analysis/history';
import {
	objectId,
	type AnalysisWarning,
	type CatalogObject,
	type ForeignKeyConstraint,
	type RelationshipEdge,
	type SchemaScope
} from '$lib/analysis/types';

/** Metadata the drafting assistant may read for one schema. Never contains row data. */
export interface AssistCatalog {
	scope: SchemaScope;
	objects: CatalogObject[];
	foreignKeys: ForeignKeyConstraint[];
	/** Relations inferred by a saved relationship analysis that covers this schema. */
	inferred: RelationshipEdge[];
	indexes: IndexInfo[];
	warnings: AnalysisWarning[];
	loadedAt: number;
}

export const CATALOG_TTL_MS = 10 * 60_000;
const HISTORY_SCAN_LIMIT = 5;

const cache = new Map<string, AssistCatalog>();

function cacheKey(scope: SchemaScope): string {
	return `${scope.connectionId}\0${scope.schema}`;
}

export function cachedAssistCatalog(scope: SchemaScope, now = Date.now()): AssistCatalog | null {
	const hit = cache.get(cacheKey(scope));
	return hit && now - hit.loadedAt < CATALOG_TTL_MS ? hit : null;
}

export function forgetAssistCatalog(connectionId: string, schema?: string) {
	for (const [key, value] of cache) {
		if (value.scope.connectionId !== connectionId) continue;
		if (schema && value.scope.schema !== schema) continue;
		cache.delete(key);
	}
}

/** Read tables, columns, keys, views and indexes, reusing a recent read of the same schema. */
export async function loadAssistCatalog(
	scope: SchemaScope,
	options: { force?: boolean } = {}
): Promise<{ catalog: AssistCatalog; cached: boolean }> {
	if (!options.force) {
		const hit = cachedAssistCatalog(scope);
		if (hit) return { catalog: hit, cached: true };
	}
	const loaded = await loadSchemaCatalog([scope], (target, sql) =>
		api.executeSql(target.connectionId, sql, target.schema, 'assist')
	);
	const objects = loaded.objects.filter((object) => !object.external);
	const [indexes, inferred] = await Promise.all([
		api.listIndexes(scope.connectionId, scope.schema).catch(() => [] as IndexInfo[]),
		savedInferredRelations(scope).catch(() => [] as RelationshipEdge[])
	]);
	const catalog: AssistCatalog = {
		scope,
		objects,
		foreignKeys: loaded.foreignKeys,
		inferred,
		indexes,
		warnings: loaded.warnings,
		loadedAt: Date.now()
	};
	cache.set(cacheKey(scope), catalog);
	return { catalog, cached: false };
}

/** Inferred edges inside `scope` from the most recent saved analysis that covers it. */
async function savedInferredRelations(scope: SchemaScope): Promise<RelationshipEdge[]> {
	const prefix = objectId(scope.connectionId, scope.schema, '');
	const summaries = (await api.listAnalysisHistory())
		.slice()
		.sort((a, b) => b.createdAt.localeCompare(a.createdAt))
		.slice(0, HISTORY_SCAN_LIMIT);
	for (const summary of summaries) {
		const record = await api.readAnalysisHistory(summary.id);
		const graph = asRelationshipGraph(record.graph);
		if (!graph?.nodes.some((node) => node.id.startsWith(prefix))) continue;
		return graph.edges.filter(
			(edge) =>
				edge.origin === 'inferred' && edge.fromId.startsWith(prefix) && edge.toId.startsWith(prefix)
		);
	}
	return [];
}
