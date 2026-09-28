import { format, type SqlLanguage } from 'sql-formatter';
import { normalizeSqlDialect } from './dialect';

export function sqlLanguageFor(engine: string | undefined | null): SqlLanguage {
	switch (normalizeSqlDialect(engine)) {
		case 'postgres':
			return 'postgresql';
		case 'oracle':
			return 'plsql';
		case 'mssql':
			return 'tsql';
		case 'mysql':
			return 'mysql';
		default:
			return 'sql';
	}
}

export function formatSql(sql: string, engine?: string | null): string {
	const trimmed = sql.trim();
	if (!trimmed) return sql;

	return format(sql, {
		language: sqlLanguageFor(engine),
		tabWidth: 2,
		keywordCase: 'upper',
		dataTypeCase: 'upper',
		functionCase: 'upper'
	});
}
