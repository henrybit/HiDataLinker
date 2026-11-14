import { describe, expect, it } from 'vitest';
import { assistExplain, isReadOnlySql } from './statement';

describe('isReadOnlySql', () => {
	it('accepts single reads', () => {
		expect(isReadOnlySql('SELECT * FROM orders;')).toBe(true);
		expect(isReadOnlySql('-- note\nWITH x AS (SELECT 1) SELECT * FROM x')).toBe(true);
		expect(isReadOnlySql("SELECT 'delete; drop' AS note")).toBe(true);
		expect(isReadOnlySql('(SELECT 1) UNION (SELECT 2)')).toBe(true);
	});

	it('treats writes, DDL and multiple statements as writes', () => {
		expect(isReadOnlySql('UPDATE orders SET amount = 0')).toBe(false);
		expect(isReadOnlySql('WITH x AS (DELETE FROM orders RETURNING id) SELECT * FROM x')).toBe(
			false
		);
		expect(isReadOnlySql('SELECT * INTO backup FROM orders')).toBe(false);
		expect(isReadOnlySql('SELECT 1; DROP TABLE orders')).toBe(false);
		expect(isReadOnlySql('CREATE TABLE t (id int)')).toBe(false);
		expect(isReadOnlySql('')).toBe(false);
	});
});

describe('assistExplain', () => {
	it('matches the statement each engine sends, without ANALYZE', () => {
		expect(assistExplain('SELECT 1;', 'postgres')).toEqual({
			statement: 'EXPLAIN (VERBOSE, COSTS, FORMAT JSON)\nSELECT 1',
			runnable: true
		});
		expect(assistExplain('SELECT 1', 'mysql')?.statement).toBe('EXPLAIN\nSELECT 1');
		expect(assistExplain('SELECT 1 FROM dual', 'oracle')?.statement).toBe(
			'EXPLAIN PLAN FOR\nSELECT 1 FROM dual'
		);
		expect(assistExplain('UPDATE t SET a = 1', 'mysql')?.runnable).toBe(true);
	});

	it('does not offer SQL Server Explain for writes, since it executes them', () => {
		expect(assistExplain('SELECT 1', 'mssql')?.runnable).toBe(true);
		expect(assistExplain('DELETE FROM t', 'mssql')?.runnable).toBe(false);
		expect(assistExplain('  ', 'mssql')).toBeNull();
	});
});
