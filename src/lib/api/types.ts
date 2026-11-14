export type EngineKind = 'mysql' | 'postgres' | 'mssql' | 'oracle';

export type ObjectKind = 'table' | 'view' | 'index' | 'trigger' | 'function' | 'procedure';

export type FolderKind = 'tables' | 'views' | 'indexes' | 'triggers' | 'functions';

export interface ConnectionProfile {
	id: string;
	name: string;
	engine: EngineKind | string;
	host: string;
	port: number;
	username: string;
	password?: string | null;
	database?: string | null;
	sslCa?: string | null;
	sslCert?: string | null;
	sslKey?: string | null;
	/** SQL Server and Oracle. False accepts a server certificate that is not trusted. */
	sslVerify?: boolean;
	/** Oracle release id such as `11.2`. Empty or `auto` negotiates the widest range. */
	oracleVersion?: string | null;
	/** `sid` connects by instance SID. Empty or `service` uses a service name. */
	oracleConnect?: string | null;
	/** Unzipped Instant Client folder that contains `oci.dll` directly. */
	oracleInstantClient?: string | null;
	savePassword: boolean;
}

export interface ConnectionListItem {
	id: string;
	name: string;
	engine: string;
	host: string;
	port: number;
	username: string;
	password?: string | null;
	database?: string | null;
	sslCa?: string | null;
	sslCert?: string | null;
	sslKey?: string | null;
	sslVerify?: boolean;
	oracleVersion?: string | null;
	oracleConnect?: string | null;
	oracleInstantClient?: string | null;
	savePassword: boolean;
	connected: boolean;
	/** Database the live session uses; may differ from `database` after a switch. */
	activeDatabase?: string | null;
}

export interface ConnectResult {
	evictedIds: string[];
}

export interface TestConnectionRequest {
	engine?: EngineKind | string;
	host: string;
	port: number;
	username: string;
	password?: string | null;
	database?: string | null;
	sslCa?: string | null;
	sslCert?: string | null;
	sslKey?: string | null;
	sslVerify?: boolean;
	oracleVersion?: string | null;
	oracleConnect?: string | null;
	oracleInstantClient?: string | null;
}

export interface DatabaseInfo {
	name: string;
	charset?: string | null;
	collation?: string | null;
	isSystem: boolean;
}

export interface CharsetInfo {
	name: string;
	defaultCollation?: string | null;
	description?: string | null;
}

export interface CollationInfo {
	name: string;
	charset: string;
	isDefault: boolean;
}

export interface CharsetCatalog {
	charsets: CharsetInfo[];
	collations: CollationInfo[];
}

export interface TableInfo {
	name: string;
	engine?: string | null;
	tableRows?: number | null;
	dataLength?: number | null;
	comment: string;
	createdAt?: string | null;
	updatedAt?: string | null;
}

export interface ViewInfo {
	name: string;
	updatable: boolean;
	checkOption?: string | null;
	securityType?: string | null;
	definer?: string | null;
}

export interface IndexInfo {
	name: string;
	tableName: string;
	unique: boolean;
	primary: boolean;
	indexType: string;
	columns: string[];
	comment: string;
}

export interface TriggerInfo {
	name: string;
	tableName: string;
	event: string;
	timing: string;
	definer?: string | null;
}

export interface RoutineInfo {
	name: string;
	routineType: string;
	returns?: string | null;
	deterministic: boolean;
	dataAccess?: string | null;
	securityType?: string | null;
	definer?: string | null;
	createdAt?: string | null;
}

export interface ColumnInfo {
	name: string;
	dataType: string;
	columnType: string;
	nullable: boolean;
	key: string;
	defaultValue?: string | null;
	extra: string;
	comment: string;
	ordinal: number;
}

export interface ColumnMeta {
	name: string;
	typeName: string;
}

export type QueryLogLevel = 'info' | 'success' | 'notice' | 'warning' | 'error';

export interface QueryLogEntry {
	level: QueryLogLevel;
	text: string;
}

/** Raw `EXPLAIN (FORMAT JSON)` document from PostgreSQL. */
export interface ExplainPlan {
	plan: string;
	analyzed: boolean;
	durationMs: number;
}

export interface QueryResult {
	columns: ColumnMeta[];
	rows: Array<Array<string | null>>;
	affectedRows: number;
	lastInsertId?: number | null;
	durationMs: number;
	truncated: boolean;
	statementKind: string;
	messages?: QueryLogEntry[];
}

export type SqlAuditClientSource = 'editor' | 'table_edit' | 'structure' | 'analysis';

export type SqlAuditSource =
	| SqlAuditClientSource
	| 'table_browse'
	| 'schema'
	| 'export'
	| 'migration'
	| 'import'
	| 'agent'
	| 'app';

export interface SqlAuditEntry {
	id: string;
	at: string;
	osUser: string;
	connectionId: string;
	connectionName: string;
	engine: string;
	host: string;
	port: number;
	username: string;
	schema: string | null;
	source: SqlAuditSource;
	sourceDetail: string | null;
	operation: string;
	sql: string;
	sqlLength: number;
	sqlTruncated: boolean;
	statementKind: string | null;
	success: boolean;
	affectedRows: number | null;
	rowCount: number | null;
	durationMs: number;
	error: string | null;
}

export interface SqlAuditSettings {
	enabled: boolean;
	retainDays: number;
}

export interface SqlAuditInfo {
	settings: SqlAuditSettings;
	dir: string;
}

export interface SqlAuditQuery {
	date?: string | null;
	connectionId?: string | null;
	source?: string | null;
	status?: 'ok' | 'error' | null;
	keyword?: string | null;
	limit?: number;
	offset?: number;
}

export interface SqlAuditPage {
	date: string;
	total: number;
	entries: SqlAuditEntry[];
}

export interface SqlAuditDay {
	date: string;
	sizeBytes: number;
}

export type TabKind = 'objects' | 'table' | 'view' | 'ddl' | 'query';

export interface Tab {
	id: string;
	kind: TabKind;
	title: string;
	connectionId: string;
	schema?: string;
	objectName?: string;
	objectKind?: ObjectKind;
	folder?: FolderKind;
	sql?: string;
	sqlCached?: boolean;
	autoRun?: boolean;
}

export interface TreeSelection {
	connectionId: string;
	schema?: string;
	folder?: FolderKind;
	objectKind?: ObjectKind;
	objectName?: string;
}

export type ContextMenuAction =
	| 'new-connection'
	| 'edit-connection'
	| 'delete-connection'
	| 'connect'
	| 'disconnect'
	| 'switch-database'
	| 'new-query'
	| 'create-database'
	| 'delete-database'
	| 'dump-database-data'
	| 'dump-database-full'
	| 'dump-table-data'
	| 'migrate-database'
	| 'run-sql-file'
	| 'refresh'
	| 'open-data'
	| 'open-structure'
	| 'open-ddl';

export interface CreateDatabasePrompt {
	connectionId: string;
	connectionName: string;
	engine: string;
	database?: string | null;
}

export interface SwitchDatabasePrompt {
	connectionId: string;
	connectionName: string;
	current: string;
	databases: string[];
}

export interface DropDatabasePrompt {
	connectionId: string;
	connectionName: string;
	engine: string;
	name: string;
}

export interface MigrateDatabasePrompt {
	connectionId: string;
	connectionName: string;
	engine: string;
	name: string;
}

export type MigrateProgressLevel = 'info' | 'success' | 'warning' | 'error';

export interface MigrateProgressEvent {
	phase: string;
	level: MigrateProgressLevel | string;
	objectKind?: string | null;
	objectName?: string | null;
	current: number;
	total: number;
	message: string;
}

export interface MigrateResult {
	sourceName: string;
	targetName: string;
	statementCount: number;
	renamed: boolean;
}

export interface AgentStatus {
	enabled: boolean;
	host: string;
	port: number;
	running: boolean;
}

export interface AgentAuditEntry {
	id: number;
	at: string;
	tool: string;
	connectionId?: string | null;
	summary: string;
	ok: boolean;
	durationMs: number;
	error?: string | null;
}

export interface BuiltinAgentInfo {
	id: string;
	name: string;
	version: string;
	kind: string;
	description: string;
}

export interface AgentSessionMeta {
	id: string;
	agentId: string;
	title: string;
	createdAt: string;
	updatedAt: string;
	status: string;
}

export interface TranscriptEntry {
	id: string;
	at: string;
	role: string;
	kind: string;
	content: string;
	meta?: Record<string, unknown> | null;
}

export interface AgentSessionDetail {
	meta: AgentSessionMeta;
	transcript: TranscriptEntry[];
	checkpoint?: unknown;
}

export interface SessionContextFile {
	summary: string;
	summarizedThroughId?: string | null;
	updatedAt: string;
	messageCountSummarized: number;
}

export interface ForceCompressResult {
	context: SessionContextFile;
	compressed: boolean;
	messagesCompressed: number;
}

export interface AgentToolResult {
	ok: boolean;
	result?: unknown;
	error?: string | null;
}

export type AttachmentKind = 'csv' | 'xlsx' | 'xls';

export interface AttachmentMeta {
	id: string;
	name: string;
	path: string;
	kind: AttachmentKind;
	size: number;
	sha256: string;
}

export interface ColumnStat {
	name: string;
	nullCount: number;
	sampleValues: string[];
}

export interface ParseSample {
	headers: string[];
	sampleRows: string[][];
	totalRows: number;
	columnCount: number;
	columnStats: ColumnStat[];
	suggestedTable: string;
	sheetName?: string | null;
	encoding?: string | null;
	delimiter?: string | null;
}

export interface SuggestedColumn {
	sourceName: string;
	name: string;
	sqlType: string;
	nullable: boolean;
}

export interface ImportScriptPackage {
	jobId: string;
	scriptDir: string;
	manifestPath: string;
	schemaSqlPath: string;
	loadSqlPath: string;
	ddl: string;
	suggestedColumns: SuggestedColumn[];
	sample: ParseSample;
	totalRows: number;
}

export interface ImportJobStatus {
	jobId: string;
	status: string;
	writtenRows: number;
	totalRows: number;
	error?: string | null;
}

export interface ImportJobProgress {
	jobId: string;
	phase: string;
	writtenRows: number;
	totalRows: number;
	message: string;
}

export interface ComposerAttachment {
	id: string;
	name: string;
	/** Local path chosen by the user (before staging). */
	sourcePath: string;
	size?: number;
}

export interface ContextMenuState {
	x: number;
	y: number;
	connectionId?: string;
	schema?: string;
	folder?: FolderKind;
	objectKind?: ObjectKind;
	objectName?: string;
	actions: ContextMenuAction[];
}

export type InsightSeverity = 'high' | 'medium' | 'low';
export type InsightCategory = 'business' | 'quality' | 'performance' | 'governance' | 'integration';

export interface InsightItem {
	id: string;
	title: string;
	body: string;
	severity: InsightSeverity;
	category: InsightCategory;
	connectionIds: string[];
	/** `schema.table` references. */
	tables: string[];
	tags: string[];
	/** Ready-to-send prompt for the data-analyst agent. */
	prompt: string;
}

export interface InsightsReport {
	generatedAt: string;
	locale: 'zh' | 'en';
	connections: { id: string; domain: string; summary: string }[];
	items: InsightItem[];
}
