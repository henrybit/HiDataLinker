import { MSSQL, MySQL, PLSQL, PostgreSQL, StandardSQL, type SQLDialect } from '@codemirror/lang-sql';

export type SqlDialectKind = 'mysql' | 'postgres' | 'mssql' | 'oracle' | 'standard';

export function normalizeSqlDialect(value: string | undefined | null): SqlDialectKind {
	const engine = (value ?? '').trim().toLowerCase();
	if (engine === 'postgres' || engine === 'postgresql' || engine === 'pgsql') return 'postgres';
	if (engine === 'oracle' || engine === 'oracledb' || engine === 'plsql') return 'oracle';
	if (
		engine === 'mssql' ||
		engine === 'sqlserver' ||
		engine === 'sql_server' ||
		engine === 'sql-server' ||
		engine === 'tsql'
	) {
		return 'mssql';
	}
	if (engine === 'mysql' || engine === 'mariadb') return 'mysql';
	return 'standard';
}

export function sqlDialectFor(engine: string | undefined | null): SQLDialect {
	switch (normalizeSqlDialect(engine)) {
		case 'postgres':
			return PostgreSQL;
		case 'oracle':
			return PLSQL;
		case 'mssql':
			return MSSQL;
		case 'mysql':
			return MySQL;
		default:
			return StandardSQL;
	}
}
