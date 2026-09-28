export type EngineKind = 'mysql' | 'postgres' | 'mssql' | 'oracle';

export const ENGINE_PRESETS = {
	mysql: { name: 'MySQL', port: 3306, username: 'root', database: '' },
	postgres: { name: 'PostgreSQL', port: 5432, username: 'postgres', database: 'postgres' },
	mssql: { name: 'SQL Server', port: 1433, username: 'sa', database: 'master' },
	oracle: { name: 'Oracle', port: 1521, username: 'system', database: 'FREEPDB1' }
} as const;

export function normalizeEngine(value: string | undefined | null): EngineKind {
	const engine = (value ?? 'mysql').trim().toLowerCase();
	if (engine === 'postgres' || engine === 'postgresql' || engine === 'pgsql') return 'postgres';
	if (engine === 'mssql' || engine === 'sqlserver' || engine === 'sql_server' || engine === 'sql-server') {
		return 'mssql';
	}
	if (engine === 'oracle' || engine === 'oracledb') return 'oracle';
	return 'mysql';
}

export function engineLabel(value: string | undefined | null): string {
	return ENGINE_PRESETS[normalizeEngine(value)].name;
}

export function isPostgres(value: string | undefined | null): boolean {
	return normalizeEngine(value) === 'postgres';
}

export function isMssql(value: string | undefined | null): boolean {
	return normalizeEngine(value) === 'mssql';
}

export function isOracle(value: string | undefined | null): boolean {
	return normalizeEngine(value) === 'oracle';
}

/** Postgres schemas and Oracle users are the tree's second level. */
export function isSchemaScoped(value: string | undefined | null): boolean {
	const engine = normalizeEngine(value);
	return engine === 'postgres' || engine === 'oracle';
}

export function isDefaultConnectionName(name: string | undefined | null): boolean {
	const trimmed = name?.trim() ?? '';
	if (!trimmed) return true;
	return Object.values(ENGINE_PRESETS).some((preset) => preset.name === trimmed);
}

export function schemaNoun(value: string | undefined | null): 'Schema' | 'Database' {
	return isSchemaScoped(value) ? 'Schema' : 'Database';
}

export function quoteIdent(engine: string | undefined | null, name: string): string {
	if (isMssql(engine)) return `[${name.replaceAll(']', ']]')}]`;
	if (isPostgres(engine) || isOracle(engine)) return `"${name.replaceAll('"', '""')}"`;
	return `\`${name.replaceAll('`', '``')}\``;
}

export function quoteLiteral(value: string | null): string {
	if (value == null) return 'NULL';
	return `'${value.replaceAll("'", "''")}'`;
}

export function qualifyIdent(engine: string | undefined | null, schema: string, name?: string): string {
	if (isMssql(engine)) {
		const database = quoteIdent(engine, schema);
		if (!name) return `${database}.`;
		const dot = name.indexOf('.');
		if (dot > 0 && !name.slice(dot + 1).includes('.')) {
			return `${database}.${quoteIdent(engine, name.slice(0, dot))}.${quoteIdent(engine, name.slice(dot + 1))}`;
		}
		return `${database}.${quoteIdent(engine, 'dbo')}.${quoteIdent(engine, name)}`;
	}
	const prefix = quoteIdent(engine, schema);
	return name ? `${prefix}.${quoteIdent(engine, name)}` : `${prefix}.`;
}
