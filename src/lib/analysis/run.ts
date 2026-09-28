import { loadSchemaCatalog, type CatalogQueryRunner } from './collect';
import { combineDocuments } from './documents';
import { inferRelationships, type StructuredCaller } from './workflow';
import type { AnalysisLocale, RelationshipGraph, SchemaScope } from './types';

export async function runRelationshipAnalysis(input: {
	scopes: SchemaScope[];
	documents: Array<{ name: string; text: string }>;
	locale: AnalysisLocale;
	caller: StructuredCaller;
	runQuery: CatalogQueryRunner;
	onProgress?: (event: {
		phase: 'catalog' | 'comments' | 'relations';
		scope?: SchemaScope;
	}) => void;
}): Promise<RelationshipGraph> {
	const catalog = await loadSchemaCatalog(input.scopes, input.runQuery, (scope) => {
		input.onProgress?.({ phase: 'catalog', scope });
	});
	return inferRelationships({
		catalog,
		documents: combineDocuments(input.documents),
		locale: input.locale,
		caller: input.caller,
		onProgress: (phase) => input.onProgress?.({ phase })
	});
}
