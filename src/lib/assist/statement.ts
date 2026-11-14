import { isMssql, isPostgres } from '$lib/engine';
import { buildExplainSql, explainTarget } from '$lib/sql/explain';
import { stripSql } from './validate';

const READ_KEYWORDS = new Set([
	'SELECT',
	'WITH',
	'SHOW',
	'DESCRIBE',
	'DESC',
	'EXPLAIN',
	'VALUES',
	'TABLE'
]);
const WRITE_IN_CTE = /\b(INSERT|UPDATE|DELETE|MERGE|TRUNCATE|DROP|ALTER|CREATE)\b/i;

/** Whether `sql` is a single statement that only reads. Anything unrecognised counts as a write. */
export function isReadOnlySql(sql: string): boolean {
	const text = stripSql(sql).trim().replace(/;\s*$/, '');
	if (!text || text.includes(';')) return false;
	const keyword = /^\(*\s*([a-z]+)/i.exec(text)?.[1]?.toUpperCase() ?? '';
	if (!READ_KEYWORDS.has(keyword)) return false;
	if (keyword === 'WITH' || keyword === 'EXPLAIN') return !WRITE_IN_CTE.test(text);
	if (keyword === 'SELECT' && /\bINTO\b/i.test(text)) return false;
	return true;
}

export interface AssistExplain {
	/** The statement the editor's Explain sends for this draft. */
	statement: string;
	/**
	 * False when explaining would run the statement itself: SQL Server's plan comes from
	 * `SET STATISTICS PROFILE ON`, which executes it.
	 */
	runnable: boolean;
}

/** The Explain the editor would send for `sql`, never with ANALYZE. */
export function assistExplain(sql: string, engine: string): AssistExplain | null {
	const target = explainTarget(sql);
	if (!target) return null;
	if (isPostgres(engine)) {
		return { statement: `EXPLAIN (VERBOSE, COSTS, FORMAT JSON)\n${target}`, runnable: true };
	}
	return {
		statement: buildExplainSql(target, engine),
		runnable: !isMssql(engine) || isReadOnlySql(target)
	};
}
