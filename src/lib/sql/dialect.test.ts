import { describe, expect, it } from 'vitest';
import { MSSQL, MySQL, PLSQL, PostgreSQL, StandardSQL } from '@codemirror/lang-sql';
import { normalizeSqlDialect, sqlDialectFor } from './dialect';

describe('sql dialect helpers', () => {
	it('normalizes engine aliases for highlighting', () => {
		expect(normalizeSqlDialect('pgsql')).toBe('postgres');
		expect(normalizeSqlDialect('PostgreSQL')).toBe('postgres');
		expect(normalizeSqlDialect('mysql')).toBe('mysql');
		expect(normalizeSqlDialect('MariaDB')).toBe('mysql');
		expect(normalizeSqlDialect('oracle')).toBe('oracle');
		expect(normalizeSqlDialect('plsql')).toBe('oracle');
		expect(normalizeSqlDialect('sqlserver')).toBe('mssql');
		expect(normalizeSqlDialect('unknown')).toBe('standard');
	});

	it('maps engines to CodeMirror dialects', () => {
		expect(sqlDialectFor('postgres')).toBe(PostgreSQL);
		expect(sqlDialectFor('mysql')).toBe(MySQL);
		expect(sqlDialectFor('oracle')).toBe(PLSQL);
		expect(sqlDialectFor('mssql')).toBe(MSSQL);
		expect(sqlDialectFor(null)).toBe(StandardSQL);
	});
});
