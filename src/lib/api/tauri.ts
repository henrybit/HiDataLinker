import { invoke } from '@tauri-apps/api/core';
import type {
	CharsetCatalog,
	ColumnInfo,
	ConnectResult,
	ConnectionListItem,
	ConnectionProfile,
	DatabaseInfo,
	IndexInfo,
	MigrateResult,
	ObjectKind,
	QueryResult,
	RoutineInfo,
	TableInfo,
	TestConnectionRequest,
	TriggerInfo,
	ViewInfo
} from './types';

export function isTauriRuntime(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
	if (!isTauriRuntime()) {
		throw new Error('Run `pnpm tauri dev` to connect to a database from the desktop app');
	}
	return invoke<T>(command, args);
}

export const api = {
	listConnections: () => call<ConnectionListItem[]>('list_connections'),

	upsertConnection: (profile: ConnectionProfile) =>
		call<ConnectionProfile>('upsert_connection', { profile }),

	deleteConnection: (id: string) => call<void>('delete_connection', { id }),

	testConnection: (request: TestConnectionRequest) => call<void>('test_connection', { request }),

	connect: (id: string, password?: string | null) =>
		call<ConnectResult>('connect_session', { id, password: password ?? null }),

	disconnect: (id: string) => call<void>('disconnect_session', { id }),

	listDatabases: (connectionId: string) => call<DatabaseInfo[]>('list_databases', { connectionId }),

	createDatabase: (
		connectionId: string,
		name: string,
		charset?: string | null,
		collation?: string | null
	) =>
		call<void>('create_database', {
			connectionId,
			name,
			charset: charset?.trim() ? charset.trim() : null,
			collation: collation?.trim() ? collation.trim() : null
		}),

	dropDatabase: (connectionId: string, name: string) =>
		call<void>('drop_database', { connectionId, name }),

	dumpDatabase: (connectionId: string, name: string, includeSchema = true, includeData = true) =>
		call<string>('dump_database', {
			connectionId,
			name,
			includeSchema,
			includeData
		}),

	dumpTable: (connectionId: string, schema: string, table: string) =>
		call<string>('dump_table', { connectionId, schema, table }),

	writeTextFile: (path: string, contents: string) =>
		call<string>('write_text_file', { path, contents }),

	writeQueryCache: (tabId: string, contents: string) =>
		call<void>('write_query_cache', { tabId, contents }),

	readQueryCache: (tabId: string) => call<string>('read_query_cache', { tabId }),

	deleteQueryCache: (tabId: string) => call<void>('delete_query_cache', { tabId }),

	migrateDatabase: (
		sourceConnectionId: string,
		sourceName: string,
		targetConnectionId: string,
		targetName: string,
		includeData = true
	) =>
		call<MigrateResult>('migrate_database', {
			sourceConnectionId,
			sourceName,
			targetConnectionId,
			targetName,
			includeData
		}),

	listCharsetCatalog: (connectionId: string) =>
		call<CharsetCatalog>('list_charset_catalog', { connectionId }),

	listTables: (connectionId: string, schema: string) =>
		call<TableInfo[]>('list_tables', { connectionId, schema }),

	listViews: (connectionId: string, schema: string) =>
		call<ViewInfo[]>('list_views', { connectionId, schema }),

	listIndexes: (connectionId: string, schema: string) =>
		call<IndexInfo[]>('list_indexes', { connectionId, schema }),

	listTriggers: (connectionId: string, schema: string) =>
		call<TriggerInfo[]>('list_triggers', { connectionId, schema }),

	listRoutines: (connectionId: string, schema: string) =>
		call<RoutineInfo[]>('list_routines', { connectionId, schema }),

	getColumns: (connectionId: string, schema: string, table: string) =>
		call<ColumnInfo[]>('get_columns', { connectionId, schema, table }),

	getDdl: (connectionId: string, schema: string, kind: ObjectKind, name: string) =>
		call<string>('get_ddl', { connectionId, schema, kind, name }),

	previewTable: (
		connectionId: string,
		schema: string,
		table: string,
		limit: number,
		offset: number
	) => call<QueryResult>('preview_table', { connectionId, schema, table, limit, offset }),

	tableRowCount: (connectionId: string, schema: string, table: string) =>
		call<number>('table_row_count', { connectionId, schema, table }),

	executeSql: (connectionId: string, sql: string, schema?: string | null) =>
		call<QueryResult>('execute_sql', { connectionId, schema: schema ?? null, sql })
};

export function errorMessage(error: unknown): string {
	if (typeof error === 'string') return error;
	if (error instanceof Error) return error.message;
	return 'Unexpected error';
}
