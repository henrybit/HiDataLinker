import type { ColumnInfo, ColumnMeta } from '$lib/api/types';
import { isMssql, isPostgres, isOracle, qualifyIdent, quoteIdent, quoteLiteral } from '$lib/engine';
import { fromInputValue, temporalKind } from './column-types';

export function isGeneratedColumn(column: ColumnInfo): boolean {
	const extra = column.extra.toLowerCase();
	return (
		extra.includes('auto_increment') ||
		extra.includes('generated always') ||
		extra.includes('identity')
	);
}

export function insertableColumns(columns: ColumnInfo[]): ColumnInfo[] {
	return columns.filter((column) => !isGeneratedColumn(column));
}

export function primaryKeyColumns(columns: ColumnInfo[]): ColumnInfo[] {
	return columns.filter((column) => column.key === 'PRI');
}

export function editableColumns(columns: ColumnInfo[]): ColumnInfo[] {
	const keys = new Set(primaryKeyColumns(columns).map((column) => column.name));
	return insertableColumns(columns).filter((column) => !keys.has(column.name));
}

function columnIndex(columns: ColumnMeta[], name: string): number {
	return columns.findIndex((column) => column.name === name);
}

function normalizeValue(column: ColumnInfo, raw: string): string {
	return fromInputValue(temporalKind(column), raw);
}

function whereClause(
	engine: string | undefined | null,
	resultColumns: ColumnMeta[],
	row: Array<string | null>,
	keys: ColumnInfo[]
): string | null {
	if (!keys.length) return null;
	const parts: string[] = [];
	for (const key of keys) {
		const index = columnIndex(resultColumns, key.name);
		if (index < 0) return null;
		const value = row[index] ?? null;
		const ident = quoteIdent(engine, key.name);
		parts.push(value == null ? `${ident} IS NULL` : `${ident} = ${quoteLiteral(value)}`);
	}
	return parts.join(' AND ');
}

function matchColumnsForRow(columns: ColumnInfo[]): ColumnInfo[] {
	const keys = primaryKeyColumns(columns);
	return keys.length ? keys : columns;
}

export function buildDeleteSql(
	engine: string | undefined | null,
	schema: string,
	table: string,
	columns: ColumnInfo[],
	resultColumns: ColumnMeta[],
	row: Array<string | null>
): string {
	const where = whereClause(engine, resultColumns, row, matchColumnsForRow(columns));
	if (!where) {
		throw new Error('Cannot build DELETE: selected row does not match table columns');
	}
	const target = qualifyIdent(engine, schema, table);
	const sql = `DELETE FROM ${target} WHERE ${where}`;
	if (isPostgres(engine) || isOracle(engine)) return sql;
	if (isMssql(engine)) return `DELETE TOP (1) FROM ${target} WHERE ${where}`;
	return `${sql} LIMIT 1`;
}

export function buildInsertSql(
	engine: string | undefined | null,
	schema: string,
	table: string,
	columns: ColumnInfo[],
	values: Record<string, string>
): string {
	const fields: string[] = [];
	const literals: string[] = [];

	for (const column of insertableColumns(columns)) {
		const raw = values[column.name];
		if (raw == null || raw === '') {
			if (!column.nullable && column.defaultValue == null) {
				throw new Error(`Column “${column.name}” is required`);
			}
			continue;
		}
		fields.push(quoteIdent(engine, column.name));
		literals.push(quoteLiteral(normalizeValue(column, raw)));
	}

	if (!fields.length) {
		throw new Error('Enter at least one column value');
	}

	const target = qualifyIdent(engine, schema, table);
	return `INSERT INTO ${target} (${fields.join(', ')}) VALUES (${literals.join(', ')})`;
}

/** Build INSERT for an existing result row (includes NULL and generated columns as displayed). */
export function buildInsertSqlFromRow(
	engine: string | undefined | null,
	schema: string,
	table: string,
	resultColumns: ColumnMeta[],
	row: Array<string | null>
): string {
	if (!resultColumns.length) {
		throw new Error('Cannot build INSERT: row has no columns');
	}
	if (row.length < resultColumns.length) {
		throw new Error('Cannot build INSERT: row does not match columns');
	}

	const fields = resultColumns.map((column) => quoteIdent(engine, column.name));
	const literals = resultColumns.map((_, index) => quoteLiteral(row[index] ?? null));
	const target = qualifyIdent(engine, schema, table);
	return `INSERT INTO ${target} (${fields.join(', ')}) VALUES (${literals.join(', ')})`;
}

export function buildUpdateSql(
	engine: string | undefined | null,
	schema: string,
	table: string,
	columns: ColumnInfo[],
	resultColumns: ColumnMeta[],
	row: Array<string | null>,
	values: Record<string, string>
): string {
	const where = whereClause(engine, resultColumns, row, matchColumnsForRow(columns));
	if (!where) {
		throw new Error('Cannot build UPDATE: selected row does not match table columns');
	}

	const sets: string[] = [];
	const targets = editableColumns(columns);
	const fallback = targets.length ? targets : insertableColumns(columns);

	for (const column of fallback) {
		const raw = values[column.name];
		const ident = quoteIdent(engine, column.name);
		if (raw == null || raw === '') {
			if (!column.nullable) {
				throw new Error(`Column “${column.name}” is required`);
			}
			sets.push(`${ident} = NULL`);
			continue;
		}
		sets.push(`${ident} = ${quoteLiteral(normalizeValue(column, raw))}`);
	}

	if (!sets.length) {
		throw new Error('No editable columns to update');
	}

	const target = qualifyIdent(engine, schema, table);
	return `UPDATE ${target} SET ${sets.join(', ')} WHERE ${where}`;
}
