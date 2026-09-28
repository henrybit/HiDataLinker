import { isMssql, isOracle, isPostgres } from '$lib/engine';

/** Strip trailing semicolons and normalize whitespace edges. */
export function normalizeExplainTarget(sql: string): string {
	return sql.trim().replace(/;+\s*$/g, '').trim();
}

/**
 * Wrap SQL with a dialect-appropriate EXPLAIN that does not execute side effects
 * (no ANALYZE), so it stays safe for DML plans.
 */
export function buildExplainSql(sql: string, engine?: string | null): string {
	let target = normalizeExplainTarget(sql);
	if (!target) return '';

	// Drop an existing EXPLAIN / EXPLAIN (...) wrapper to avoid nesting.
	target = target.replace(/^\s*EXPLAIN\s*(?:\([^)]*\))?\s*/i, '').trim();
	if (!target) return '';

	if (isPostgres(engine)) {
		return `EXPLAIN (VERBOSE, COSTS, FORMAT TEXT)\n${target}`;
	}
	if (isOracle(engine)) {
		return `EXPLAIN PLAN FOR\n${target}`;
	}
	if (isMssql(engine)) {
		return `SET STATISTICS PROFILE ON;\n${target}`;
	}

	return `EXPLAIN\n${target}`;
}
