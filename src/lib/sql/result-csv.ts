import type { QueryResult } from '$lib/api/types';

function cell(value: string | null): string {
	if (value == null) return '';
	return /[",\r\n]/.test(value) || /^\s|\s$/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

/**
 * RFC 4180 CSV of a result grid: header row, CRLF line ends, NULL as an empty field. Starts with
 * a BOM so spreadsheet apps read UTF-8 (CJK) text correctly.
 */
export function resultToCsv(result: Pick<QueryResult, 'columns' | 'rows'>): string {
	const lines = [result.columns.map((column) => cell(column.name)).join(',')];
	for (const row of result.rows) lines.push(row.map(cell).join(','));
	return `\uFEFF${lines.join('\r\n')}\r\n`;
}

export function csvFileName(base: string): string {
	const safe = base.trim().replace(/[\\/:*?"<>|\s]+/g, '_') || 'result';
	return `${safe}.csv`;
}
