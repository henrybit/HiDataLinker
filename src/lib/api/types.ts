export type EngineKind = 'mysql' | 'postgres';

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
	savePassword: boolean;
	connected: boolean;
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
