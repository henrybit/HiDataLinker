import type { ParseSample } from '$lib/api/types';

const VARCHAR_RE = /\b(?:CHARACTER\s+VARYING|VARCHAR)\s*\(\s*\d+\s*\)/gi;

const LENGTH_ERROR_RE =
	/value too long for type character varying|character varying\(\d+\)|22001|data too long for column|1406/i;

const CREATE_TABLE_RE =
	/CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?((?:"[^"]+"|`[^`]+`|\[[^\]]+\]|[A-Za-z_][\w$]*)(?:\s*\.\s*(?:"[^"]+"|`[^`]+`|\[[^\]]+\]|[A-Za-z_][\w$]*))?)/i;

/** Drop a single surrounding markdown fence if the model wrapped the DDL. */
export function stripSqlFence(sql: string): string {
	const trimmed = sql.trim();
	const fenced = trimmed.match(/^```(?:sql)?\s*([\s\S]*?)```$/i);
	return (fenced?.[1] ?? trimmed).trim();
}

/** Schema statements only. The load plan after this marker is a comment, not executed SQL. */
export function schemaSqlFromPreview(preview: string): string {
	const marker = preview.search(/\n-- Import load plan\b/);
	const sql = marker === -1 ? preview : preview.slice(0, marker);
	return sql.trim();
}

/** Sample-only lengths. The full file can be longer than these. */
export function columnLengthHints(sample: ParseSample | null): string {
	if (!sample) return '';
	return sample.headers
		.map((header, index) => {
			let max = 0;
			let nonEmpty = 0;
			for (const row of sample.sampleRows) {
				const value = row[index] ?? '';
				if (!value) continue;
				nonEmpty += 1;
				max = Math.max(max, [...value].length);
			}
			return `- ${header}: sampleMaxChars=${max}, nonEmptySampleRows=${nonEmpty}`;
		})
		.join('\n');
}

export function widenStringColumns(sql: string): string {
	return sql.replace(VARCHAR_RE, 'TEXT');
}

/** So a corrected CREATE replaces the table left behind by a failed import. */
export function ensureDropBeforeCreate(sql: string): string {
	const trimmed = sql.trim();
	if (/\bDROP\s+TABLE\b/i.test(trimmed)) return trimmed;
	const match = trimmed.match(CREATE_TABLE_RE);
	if (!match?.[1]) return trimmed;
	const name = match[1].replace(/\s+/g, '');
	return `DROP TABLE IF EXISTS ${name};\n${trimmed}`;
}

/**
 * Deterministic repair for "value too long for varchar(n)".
 * Returns null when the error is not a string-length failure.
 */
export function repairImportSqlLocally(
	sql: string,
	error: string
): { sql: string } | null {
	if (!sql.trim() || !LENGTH_ERROR_RE.test(error)) return null;
	const widened = widenStringColumns(sql);
	if (!/\bTEXT\b/i.test(widened) && widened === sql) return null;
	let next = ensureDropBeforeCreate(widened);
	if (!next.endsWith(';')) next = `${next};`;
	if (next.trim() === sql.trim()) return null;
	return { sql: next };
}

export function importSqlRepairPrompt(input: {
	engine: string;
	error: string;
	sql: string;
	columnHints: string;
	locale: 'zh' | 'en';
}): { system: string; human: string } {
	const system =
		input.locale === 'zh'
			? '你是数据库导入脚本纠错助手。只根据报错修改 DDL，保持表名和列名不变。若错误是字符串超出 varchar 长度，把相关字符串列改为 TEXT。失败的导入可能已经建好表，所以在 CREATE TABLE 之前加上 DROP TABLE IF EXISTS（同一张表）。不要输出 INSERT，不要解释过程。'
			: 'You fix import DDL from the database error. Keep the table name and column names. If the error is a string exceeding varchar, change those string columns to TEXT. A failed import may have already created the table, so put DROP TABLE IF EXISTS for that same table before CREATE TABLE. Do not emit INSERT statements or extra commentary.';
	const human = [
		`Engine: ${input.engine || 'postgres'}`,
		`Error:\n${input.error}`,
		`Current SQL:\n${input.sql}`,
		input.columnHints
			? `Sample column lengths (the full file can be longer):\n${input.columnHints}`
			: ''
	]
		.filter(Boolean)
		.join('\n\n');
	return { system, human };
}
