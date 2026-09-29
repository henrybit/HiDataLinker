import { loadSchemaCatalog, type CatalogQueryRunner } from './collect';
import { combineDocuments } from './documents';
import type { AnalysisLogEvent } from './log';
import { inferRelationships, type StructuredCaller } from './workflow';
import type { AnalysisLocale, RelationshipGraph, SchemaScope } from './types';

export async function runRelationshipAnalysis(input: {
	scopes: SchemaScope[];
	documents: Array<{ name: string; text: string }>;
	locale: AnalysisLocale;
	caller: StructuredCaller;
	runQuery: CatalogQueryRunner;
	onLog?: (event: AnalysisLogEvent) => void;
}): Promise<RelationshipGraph> {
	const catalog = await loadSchemaCatalog(input.scopes, input.runQuery, input.onLog);
	return inferRelationships({
		catalog,
		documents: combineDocuments(input.documents),
		locale: input.locale,
		caller: input.caller,
		onLog: input.onLog
	});
}
