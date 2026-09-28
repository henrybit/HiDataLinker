import { describe, expect, it } from 'vitest';
import { buildExplainSql, normalizeExplainTarget } from './explain';

describe('explain sql helpers', () => {
	it('normalizes trailing semicolons', () => {
		expect(normalizeExplainTarget('SELECT 1;;;')).toBe('SELECT 1');
	});

	it('builds postgres explain without analyze', () => {
		expect(buildExplainSql('SELECT * FROM users;', 'postgres')).toBe(
			'EXPLAIN (VERBOSE, COSTS, FORMAT TEXT)\nSELECT * FROM users'
		);
	});

	it('builds mysql explain', () => {
		expect(buildExplainSql('SELECT id FROM t', 'mysql')).toBe('EXPLAIN\nSELECT id FROM t');
	});

	it('rewraps sql that already starts with explain', () => {
		expect(buildExplainSql('EXPLAIN SELECT 1', 'mysql')).toBe('EXPLAIN\nSELECT 1');
		expect(buildExplainSql('EXPLAIN (ANALYZE) SELECT 1', 'postgres')).toBe(
			'EXPLAIN (VERBOSE, COSTS, FORMAT TEXT)\nSELECT 1'
		);
	});

	it('builds sql server and oracle explain', () => {
		expect(buildExplainSql('SELECT id FROM t', 'mssql')).toBe(
			'SET STATISTICS PROFILE ON;\nSELECT id FROM t'
		);
		expect(buildExplainSql('SELECT id FROM t', 'oracle')).toBe('EXPLAIN PLAN FOR\nSELECT id FROM t');
	});

	it('returns empty for blank input', () => {
		expect(buildExplainSql('   ', 'mysql')).toBe('');
	});
});
