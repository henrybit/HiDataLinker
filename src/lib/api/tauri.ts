import { invoke } from '@tauri-apps/api/core';
import { oracleConnectLogDetail } from '$lib/connection-test';
import { isOracle } from '$lib/engine';
import type {
	AnalysisHistoryRecord,
	AnalysisHistorySummary,
	NewAnalysisHistory
} from '$lib/analysis/history';
import type {
	MigrationHistoryRecord,
	MigrationHistorySummary,
	NewMigrationHistory
} from '$lib/migration/history';
import type {
	CharsetCatalog,
	ColumnInfo,
	AgentAuditEntry,
	AgentStatus,
	ConnectResult,
	ConnectionListItem,
	ConnectionProfile,
	DatabaseInfo,
	ExplainPlan,
	IndexInfo,
	MigrateResult,
	ObjectKind,
	QueryResult,
	RoutineInfo,
	SchemaHealthReport,
	SchemaObjectInfo,
	SqlAuditClientSource,
	SqlAuditDay,
	SqlAuditInfo,
	SqlAuditPage,
	SqlAuditQuery,
	SqlAuditSettings,
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
	agentStatus: () => call<AgentStatus>('agent_status'),

	setAgentEnabled: (enabled: boolean) => call<AgentStatus>('set_agent_enabled', { enabled }),

	listAgentAudit: () => call<AgentAuditEntry[]>('list_agent_audit'),

	listConnections: () => call<ConnectionListItem[]>('list_connections'),

	upsertConnection: (profile: ConnectionProfile) =>
		call<ConnectionProfile>('upsert_connection', { profile }),

	deleteConnection: (id: string) => call<void>('delete_connection', { id }),

	testConnection: async (request: TestConnectionRequest) => {
		const oracle = isOracle(request.engine);
		if (oracle) console.info(`[oracle] test connection ${oracleConnectLogDetail(request)}`);
		try {
			return await call<void>('test_connection', { request });
		} catch (error) {
			if (oracle) {
				console.error(
					`[oracle] test connection failed ${oracleConnectLogDetail(request)} | ${errorMessage(error)}`,
					error
				);
			}
			throw error;
		}
	},

	connect: (id: string, password?: string | null) =>
		call<ConnectResult>('connect_session', { id, password: password ?? null }),

	disconnect: (id: string) => call<void>('disconnect_session', { id }),

	listServerDatabases: (connectionId: string) =>
		call<string[]>('list_server_databases', { connectionId }),

	switchDatabase: (connectionId: string, database: string) =>
		call<void>('switch_database', { connectionId, database }),

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

	writeBinaryFile: (path: string, contents: Uint8Array) =>
		call<string>('write_binary_file', { path, contents: Array.from(contents) }),

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

	schemaHealth: (connectionId: string, schema: string) =>
		call<SchemaHealthReport>('schema_health', { connectionId, schema }),

	listSchemaObjects: (connectionId: string, schema: string, kind: ObjectKind) =>
		call<SchemaObjectInfo[]>('list_schema_objects', { connectionId, schema, kind }),

	previewTable: (
		connectionId: string,
		schema: string,
		table: string,
		limit: number,
		offset: number
	) => call<QueryResult>('preview_table', { connectionId, schema, table, limit, offset }),

	tableRowCount: (connectionId: string, schema: string, table: string) =>
		call<number>('table_row_count', { connectionId, schema, table }),

	executeSql: (
		connectionId: string,
		sql: string,
		schema?: string | null,
		source: SqlAuditClientSource = 'editor'
	) => call<QueryResult>('execute_sql', { connectionId, schema: schema ?? null, sql, source }),

	explainPlan: (
		connectionId: string,
		sql: string,
		schema: string | null | undefined,
		analyze: boolean,
		source: SqlAuditClientSource = 'editor'
	) =>
		call<ExplainPlan>('explain_plan', {
			connectionId,
			schema: schema ?? null,
			sql,
			analyze,
			source
		}),

	readLlmCatalog: () => call<string>('read_llm_catalog'),

	writeLlmCatalog: (contents: string) => call<void>('write_llm_catalog', { contents }),

	readLocale: () => call<string>('read_locale'),

	writeLocale: (locale: 'en' | 'zh') => call<void>('write_locale', { locale }),

	listAnalysisHistory: () => call<AnalysisHistorySummary[]>('list_analysis_history'),

	readAnalysisHistory: (id: string) => call<AnalysisHistoryRecord>('read_analysis_history', { id }),

	saveAnalysisHistory: (request: NewAnalysisHistory) =>
		call<AnalysisHistorySummary>('save_analysis_history', { request }),

	deleteAnalysisHistory: (id: string) => call<void>('delete_analysis_history', { id }),

	listMigrationHistory: () => call<MigrationHistorySummary[]>('list_migration_history'),

	readMigrationHistory: (id: string) =>
		call<MigrationHistoryRecord>('read_migration_history', { id }),

	saveMigrationHistory: (request: NewMigrationHistory) =>
		call<MigrationHistorySummary>('save_migration_history', { request }),

	deleteMigrationHistory: (id: string) => call<void>('delete_migration_history', { id }),

	completeLlm: (request: {
		provider: 'openai' | 'anthropic';
		apiKey: string;
		model: string;
		baseUrl: string;
		system: string;
		human: string;
		schema: Record<string, unknown>;
	}) => call<unknown>('complete_llm', { request }),

	startLlm: (request: {
		provider: 'openai' | 'anthropic';
		apiKey: string;
		model: string;
		baseUrl: string;
		system: string;
		human: string;
		schema: Record<string, unknown>;
	}) => call<string>('start_llm', { request }),

	pollLlm: (id: string) =>
		call<{ status: 'pending' | 'done' | 'failed'; result?: unknown; error?: string | null }>(
			'poll_llm',
			{ id }
		),

	agentsList: () => call<import('./types').BuiltinAgentInfo[]>('agents_list'),

	agentReadSystem: (agentId: string) => call<string>('agent_read_system', { agentId }),

	agentListSessions: (agentId: string) =>
		call<import('./types').AgentSessionMeta[]>('agent_list_sessions', { agentId }),

	agentCreateSession: (agentId: string, request: { title: string }) =>
		call<import('./types').AgentSessionMeta>('agent_create_session', { agentId, request }),

	agentLoadSession: (agentId: string, sessionId: string) =>
		call<import('./types').AgentSessionDetail>('agent_load_session', { agentId, sessionId }),

	agentAppendTranscript: (
		agentId: string,
		sessionId: string,
		entry: import('./types').TranscriptEntry
	) => call<import('./types').TranscriptEntry>('agent_append_transcript', { agentId, sessionId, entry }),

	agentSaveCheckpoint: (
		agentId: string,
		sessionId: string,
		checkpoint: unknown,
		status?: string | null
	) =>
		call<void>('agent_save_checkpoint', {
			agentId,
			sessionId,
			checkpoint,
			status: status ?? null
		}),

	agentReadContext: (agentId: string, sessionId: string) =>
		call<import('./types').SessionContextFile>('agent_read_context', { agentId, sessionId }),

	agentCompressContext: (request: {
		agentId: string;
		sessionId: string;
		locale: 'zh' | 'en';
		llm: {
			provider: string;
			apiKey: string;
			model: string;
			baseUrl: string;
		};
	}) => call<import('./types').ForceCompressResult>('agent_compress_context', { request }),

	agentToolCall: (name: string, args: Record<string, unknown> = {}) =>
		call<import('./types').AgentToolResult>('agent_tool_call', { name, args }),

	agentStartRun: (request: {
		agentId: string;
		sessionId: string;
		query: string;
		systemMd: string;
		locale: 'zh' | 'en';
		/** Optional @-mentioned data sources; empty = all connections. */
		connectionIds?: string[];
		/** Optional /-selected skills; empty = all skills/tools. */
		skillIds?: string[];
		/** Authorization mode for SQL tools on this run. */
		authMode?: 'readonly' | 'full';
		/** Staged tabular attachments with parse samples for import analysis. */
		attachments?: Array<{
			id: string;
			name: string;
			path: string;
			kind: string;
			size: number;
			sha256: string;
			sample?: import('./types').ParseSample | null;
		}>;
		llm: {
			provider: string;
			apiKey: string;
			model: string;
			baseUrl: string;
		};
	}) => call<{ runId: string }>('agent_start_run', { request }),

	agentCancelRun: (runId: string) => call<void>('agent_cancel_run', { runId }),

	insightsStartRun: (request: {
		locale: 'zh' | 'en';
		/** Empty = every saved connection. */
		connectionIds?: string[];
		llm: {
			provider: string;
			apiKey: string;
			model: string;
			baseUrl: string;
		};
	}) => call<{ runId: string }>('insights_start_run', { request }),

	insightsReadLatest: () =>
		call<import('./types').InsightsReport | null>('insights_read_latest'),

	agentRunnerStatus: () =>
		call<{
			ready: boolean;
			activeRuns: number;
			scriptPath?: string | null;
			nodeBin?: string | null;
		}>('agent_runner_status'),

	agentStageAttachments: (request: {
		agentId: string;
		sessionId: string;
		files: Array<{ sourcePath: string; name?: string }>;
	}) => call<import('./types').AttachmentMeta[]>('agent_stage_attachments', { request }),

	importParseSample: (request: { path: string; name: string }) =>
		call<import('./types').ParseSample>('import_parse_sample', { request }),

	importInferSchema: (
		request: { sample: import('./types').ParseSample; engine: string },
		schema?: string,
		table?: string
	) =>
		call<{
			columns: import('./types').SuggestedColumn[];
			ddl: string;
			table: string;
			schema: string;
		}>('import_infer_schema', { request, schema, table }),

	importPrepare: (request: {
		agentId: string;
		sessionId: string;
		attachmentPath: string;
		attachmentName: string;
		connectionId: string;
		engine: string;
		schema: string;
		table: string;
		mode: string;
		createTable: boolean;
		ddl?: string;
		columnMappings?: Array<{ source: string; target: string }>;
	}) => call<import('./types').ImportScriptPackage>('import_prepare', { request }),

	importStart: (request: { jobId: string; scriptDir: string }) =>
		call<import('./types').ImportJobStatus>('import_start', { request }),

	importCancel: (jobId: string) => call<void>('import_cancel', { jobId }),

	importStatus: (jobId: string) =>
		call<import('./types').ImportJobStatus | null>('import_status', { jobId }),

	importReadTextFile: (path: string) => call<string>('import_read_text_file', { path }),

	readRuntimeSettings: () =>
		call<{ fileLoggingEnabled: boolean }>('read_runtime_settings'),

	writeRuntimeSettings: (settings: { fileLoggingEnabled: boolean }) =>
		call<{ fileLoggingEnabled: boolean }>('write_runtime_settings', { settings }),

	readSqlAuditSettings: () => call<SqlAuditInfo>('read_sql_audit_settings'),

	writeSqlAuditSettings: (settings: SqlAuditSettings) =>
		call<SqlAuditInfo>('write_sql_audit_settings', { settings }),

	listSqlAudit: (query: SqlAuditQuery) => call<SqlAuditPage>('list_sql_audit', { query }),

	listSqlAuditDays: () => call<SqlAuditDay[]>('list_sql_audit_days')
};

export function errorMessage(error: unknown): string {
	if (typeof error === 'string') return error;
	if (error instanceof Error) return error.message;
	return 'Unexpected error';
}
