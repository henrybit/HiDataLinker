import type { QueryResult } from '$lib/api/types';
import { catalogSql } from './catalog-sql';
import type { AnalysisLogEvent, CatalogQueryName } from './log';
import {
	objectId,
	type AnalysisWarning,
	type CatalogObject,
	type ForeignKeyConstraint,
	type SchemaCatalog,
	type SchemaScope
} from './types';

export interface CatalogQueryRunner {
	(scope: SchemaScope, sql: string): Promise<QueryResult>;
}

export async function loadSchemaCatalog(
	scopes: SchemaScope[],
	runQuery: CatalogQueryRunner,
	onLog?: (event: AnalysisLogEvent) => void
): Promise<SchemaCatalog> {
	const loaded: ScopeDump[] = [];
	for (const scope of scopes) {
		const name = `${scope.connectionName}.${scope.schema}`;
		onLog?.({ type: 'catalog-scope', name });
		const sql = catalogSql(scope.engine, scope.schema, scope.oracleVersion);
		const timed = (query: CatalogQueryName, statement: string) =>
			runCatalogQuery(scope, query, statement, runQuery, onLog);
		loaded.push({
			scope,
			objects: await timed('objects', sql.objects),
			columns: await timed('columns', sql.columns),
			foreignKeys: await timed('foreignKeys', sql.foreignKeys),
			views: await timed('views', sql.views)
		});
	}
	const catalog = catalogFromDumps(loaded);
	onLog?.({
		type: 'catalog-done',
		objects: catalog.objects.filter((object) => !object.external).length,
		foreignKeys: catalog.foreignKeys.length
	});
	for (const warning of catalog.warnings) {
		onLog?.({
			type: 'catalog-warning',
			code: warning.code,
			name: `${warning.connectionName}.${warning.schema}`
		});
	}
	return catalog;
}

async function runCatalogQuery(
	scope: SchemaScope,
	query: CatalogQueryName,
	statement: string,
	runQuery: CatalogQueryRunner,
	onLog?: (event: AnalysisLogEvent) => void
): Promise<QueryResult> {
	const started = performance.now();
	const result = await runQuery(scope, statement);
	onLog?.({
		type: 'catalog-query',
		name: `${scope.connectionName}.${scope.schema}`,
		query,
		rows: result.rows.length,
		durationMs: Math.round(performance.now() - started),
		truncated: result.truncated
	});
	return result;
}

interface ScopeDump {
	scope: SchemaScope;
	objects: QueryResult;
	columns: QueryResult;
	foreignKeys: QueryResult;
	views: QueryResult;
}

interface ForeignKeyDraft {
	name: string;
	scope: SchemaScope;
	fromTable: string;
	toSchema: string;
	toTable: string;
	pairs: Array<{ fromColumn: string; toColumn: string; ordinal: number }>;
}

export function catalogFromDumps(dumps: ScopeDump[]): SchemaCatalog {
	const objects: CatalogObject[] = [];
	const warnings: AnalysisWarning[] = [];
	const drafts: ForeignKeyDraft[] = [];

	for (const dump of dumps) {
		if (
			dump.objects.truncated ||
			dump.columns.truncated ||
			dump.foreignKeys.truncated ||
			dump.views.truncated
		) {
			warnings.push({
				code: 'truncated',
				connectionName: dump.scope.connectionName,
				schema: dump.scope.schema
			});
		}
		for (const row of records(dump.objects)) {
			const name = row.object_name;
			if (!name) continue;
			upsertObject(
				objects,
				dump.scope,
				name,
				row.object_kind === 'view' ? 'view' : 'table',
				row.comment
			);
		}
		for (const row of records(dump.columns)) {
			const name = row.object_name;
			const columnName = row.column_name;
			if (!name || !columnName) continue;
			const object = upsertObject(objects, dump.scope, name, 'table', '');
			object.columns.push({
				name: columnName,
				dataType: row.column_type,
				key: row.column_key,
				comment: row.comment,
				ordinal: Number(row.ordinal_position) || object.columns.length + 1
			});
		}
		for (const row of records(dump.views)) {
			const name = row.object_name;
			if (!name) continue;
			const object = upsertObject(objects, dump.scope, name, 'view', '');
			object.kind = 'view';
			object.definition = row.definition.slice(0, 100_000);
		}
		const grouped = new Map<string, ForeignKeyDraft>();
		for (const row of records(dump.foreignKeys)) {
			if (!row.from_table || !row.to_table || !row.from_column || !row.to_column) continue;
			const toSchema = row.to_schema || dump.scope.schema;
			const key = [
				dump.scope.connectionId,
				dump.scope.schema,
				row.constraint_name,
				row.from_table,
				toSchema,
				row.to_table
			].join('\0');
			const draft = grouped.get(key) ?? {
				name: row.constraint_name || 'fk',
				scope: dump.scope,
				fromTable: row.from_table,
				toSchema,
				toTable: row.to_table,
				pairs: []
			};
			draft.pairs.push({
				fromColumn: row.from_column,
				toColumn: row.to_column,
				ordinal: Number(row.ordinal_position) || draft.pairs.length + 1
			});
			grouped.set(key, draft);
		}
		drafts.push(...grouped.values());
	}

	for (const object of objects) {
		object.columns.sort((a, b) => a.ordinal - b.ordinal);
	}

	const foreignKeys: ForeignKeyConstraint[] = drafts.map((draft) => {
		const from = ensureObject(objects, draft.scope, draft.scope.schema, draft.fromTable);
		const toScope = { ...draft.scope, schema: draft.toSchema };
		const to = ensureObject(objects, toScope, draft.toSchema, draft.toTable);
		const pairs = [...draft.pairs].sort((a, b) => a.ordinal - b.ordinal);
		return {
			id: `${from.id}::${draft.name}::${to.id}`,
			name: draft.name,
			connectionId: draft.scope.connectionId,
			fromId: from.id,
			toId: to.id,
			pairs: pairs.map((pair) => ({ fromColumn: pair.fromColumn, toColumn: pair.toColumn }))
		};
	});

	const realObjects = objects.filter((object) => !object.external);
	if (dumps.length > 0 && realObjects.length === 0) {
		warnings.push({
			code: 'empty',
			connectionName: dumps[0].scope.connectionName,
			schema: dumps[0].scope.schema
		});
	}

	return { objects, foreignKeys, warnings };
}

function upsertObject(
	objects: CatalogObject[],
	scope: SchemaScope,
	name: string,
	kind: CatalogObject['kind'],
	comment: string
): CatalogObject {
	const found = findObject(objects, scope.connectionId, scope.schema, name);
	if (found) {
		if (found.external) {
			found.external = false;
			found.kind = kind;
			found.connectionName = scope.connectionName;
			found.engine = scope.engine;
		}
		if (!found.comment && comment) found.comment = comment;
		if (kind === 'view') found.kind = 'view';
		return found;
	}
	const created = createObject(scope, scope.schema, name, kind, comment, false);
	objects.push(created);
	return created;
}

function ensureObject(
	objects: CatalogObject[],
	scope: SchemaScope,
	schema: string,
	name: string
): CatalogObject {
	const found = findObject(objects, scope.connectionId, schema, name);
	if (found) return found;
	const created = createObject({ ...scope, schema }, schema, name, 'table', '', true);
	objects.push(created);
	return created;
}

function createObject(
	scope: SchemaScope,
	schema: string,
	name: string,
	kind: CatalogObject['kind'],
	comment: string,
	external: boolean
): CatalogObject {
	return {
		id: objectId(scope.connectionId, schema, name),
		connectionId: scope.connectionId,
		connectionName: scope.connectionName,
		engine: scope.engine,
		schema,
		name,
		kind,
		comment,
		definition: '',
		columns: [],
		external
	};
}

function findObject(
	objects: CatalogObject[],
	connectionId: string,
	schema: string,
	name: string
): CatalogObject | undefined {
	const id = objectId(connectionId, schema, name);
	return (
		objects.find((object) => object.id === id) ??
		objects.find((object) => object.id.toLowerCase() === id.toLowerCase())
	);
}

function records(result: QueryResult): Array<Record<string, string>> {
	if (result.rows.length === 0) return [];
	if (result.columns.length === 0) {
		throw new Error('Catalog query returned rows without column names');
	}
	const names = result.columns.map((column) => column.name.toLowerCase());
	return result.rows.map((row) => {
		const record: Record<string, string> = {};
		names.forEach((name, index) => {
			record[name] = row[index] ?? '';
		});
		return record;
	});
}
