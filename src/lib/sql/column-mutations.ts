import type { ColumnInfo } from '$lib/api/types';
import { isMssql, isOracle, isPostgres, qualifyIdent, quoteIdent, quoteLiteral } from '$lib/engine';
import { isGeneratedColumn } from './row-mutations';

export type ColumnSpec = {
	name: string;
	columnType: string;
	nullable: boolean;
	/** Empty string means no default. */
	defaultValue: string;
	comment: string;
};

export function emptyColumnSpec(): ColumnSpec {
	return {
		name: '',
		columnType: '',
		nullable: true,
		defaultValue: '',
		comment: ''
	};
}

export function columnSpecFromInfo(column: ColumnInfo): ColumnSpec {
	return {
		name: column.name,
		columnType: column.columnType || column.dataType,
		nullable: column.nullable,
		defaultValue: column.defaultValue ?? '',
		comment: column.comment ?? ''
	};
}

function tableRef(engine: string | undefined | null, schema: string, table: string): string {
	return qualifyIdent(engine, schema, table);
}

/** Format a DEFAULT clause value: keep expressions, quote plain strings. */
export function formatDefaultExpr(raw: string): string {
	const value = raw.trim();
	if (!value) return '';
	if (
		/^(null|true|false|current_timestamp|current_date|current_time|localtimestamp|localtime|now\(\))$/i.test(
			value
		)
	) {
		return value;
	}
	if (/^-?\d+(\.\d+)?$/.test(value)) return value;
	if (value.includes('(') || value.includes('::')) return value;
	if (
		(value.startsWith("'") && value.endsWith("'")) ||
		(value.startsWith('"') && value.endsWith('"'))
	) {
		return value;
	}
	return quoteLiteral(value);
}

function mysqlColumnClause(spec: ColumnSpec, extra = ''): string {
	const type = spec.columnType.trim();
	if (!spec.name.trim() || !type) {
		throw new Error('Column name and type are required');
	}
	const parts = [quoteIdent('mysql', spec.name.trim()), type];
	parts.push(spec.nullable ? 'NULL' : 'NOT NULL');
	const def = formatDefaultExpr(spec.defaultValue);
	if (def) parts.push(`DEFAULT ${def}`);
	const extraLower = extra.toLowerCase();
	if (extraLower.includes('auto_increment')) parts.push('AUTO_INCREMENT');
	if (spec.comment.trim()) parts.push(`COMMENT ${quoteLiteral(spec.comment.trim())}`);
	return parts.join(' ');
}

function commentSql(
	engine: string | undefined | null,
	schema: string,
	table: string,
	column: string,
	comment: string
): string {
	const target = `${qualifyIdent(engine, schema, table)}.${quoteIdent(engine, column)}`;
	if (!comment.trim()) return `COMMENT ON COLUMN ${target} IS NULL`;
	return `COMMENT ON COLUMN ${target} IS ${quoteLiteral(comment.trim())}`;
}

function mssqlParts(table: string): { schema: string; name: string } {
	const dot = table.indexOf('.');
	if (dot > 0 && !table.slice(dot + 1).includes('.')) {
		return { schema: table.slice(0, dot), name: table.slice(dot + 1) };
	}
	return { schema: 'dbo', name: table };
}

function nLiteral(value: string): string {
	return `N${quoteLiteral(value)}`;
}

export function buildAddColumnSql(
	engine: string | undefined | null,
	schema: string,
	table: string,
	spec: ColumnSpec
): string[] {
	const name = spec.name.trim();
	const type = spec.columnType.trim();
	if (!name || !type) throw new Error('Column name and type are required');

	const target = tableRef(engine, schema, table);
	if (isPostgres(engine)) {
		const parts = [`ADD COLUMN ${quoteIdent(engine, name)} ${type}`];
		parts.push(spec.nullable ? 'NULL' : 'NOT NULL');
		const def = formatDefaultExpr(spec.defaultValue);
		if (def) parts.push(`DEFAULT ${def}`);
		const sql = [`ALTER TABLE ${target} ${parts.join(' ')}`];
		if (spec.comment.trim()) sql.push(commentSql(engine, schema, table, name, spec.comment));
		return sql;
	}
	if (isOracle(engine)) {
		const parts = [`${quoteIdent(engine, name)} ${type}`];
		parts.push(spec.nullable ? 'NULL' : 'NOT NULL');
		const def = formatDefaultExpr(spec.defaultValue);
		if (def) parts.push(`DEFAULT ${def}`);
		const sql = [`ALTER TABLE ${target} ADD (${parts.join(' ')})`];
		if (spec.comment.trim()) sql.push(commentSql(engine, schema, table, name, spec.comment));
		return sql;
	}
	if (isMssql(engine)) {
		const parts = [`${quoteIdent(engine, name)} ${type}`, spec.nullable ? 'NULL' : 'NOT NULL'];
		const def = formatDefaultExpr(spec.defaultValue);
		if (def) parts.push(`CONSTRAINT ${quoteIdent(engine, `DF_${name}`)} DEFAULT ${def}`);
		const sql = [`ALTER TABLE ${target} ADD ${parts.join(' ')}`];
		if (spec.comment.trim()) sql.push(mssqlColumnComment(table, name, spec.comment, false));
		return sql;
	}

	return [`ALTER TABLE ${target} ADD COLUMN ${mysqlColumnClause(spec)}`];
}

export function buildDropColumnSql(
	engine: string | undefined | null,
	schema: string,
	table: string,
	columnName: string
): string {
	const name = columnName.trim();
	if (!name) throw new Error('Column name is required');
	return `ALTER TABLE ${tableRef(engine, schema, table)} DROP COLUMN ${quoteIdent(engine, name)}`;
}

function specsEqual(a: ColumnSpec, b: ColumnSpec): boolean {
	return (
		a.name.trim() === b.name.trim() &&
		a.columnType.trim() === b.columnType.trim() &&
		a.nullable === b.nullable &&
		a.defaultValue.trim() === b.defaultValue.trim() &&
		a.comment.trim() === b.comment.trim()
	);
}

export function buildAlterColumnSql(
	engine: string | undefined | null,
	schema: string,
	table: string,
	before: ColumnInfo,
	after: ColumnSpec
): string[] {
	const next = {
		...after,
		name: after.name.trim(),
		columnType: after.columnType.trim(),
		defaultValue: after.defaultValue.trim(),
		comment: after.comment.trim()
	};
	if (!next.name || !next.columnType) throw new Error('Column name and type are required');

	const prev = columnSpecFromInfo(before);
	if (specsEqual(prev, next)) return [];

	if (isGeneratedColumn(before) && (prev.columnType !== next.columnType || prev.name !== next.name)) {
		throw new Error('Generated / identity columns cannot change name or type here');
	}

	const target = tableRef(engine, schema, table);

	if (isPostgres(engine)) {
		const sql: string[] = [];
		let workingName = before.name;

		if (prev.name !== next.name) {
			sql.push(
				`ALTER TABLE ${target} RENAME COLUMN ${quoteIdent(engine, before.name)} TO ${quoteIdent(engine, next.name)}`
			);
			workingName = next.name;
		}

		const col = quoteIdent(engine, workingName);
		if (prev.columnType !== next.columnType) {
			sql.push(`ALTER TABLE ${target} ALTER COLUMN ${col} TYPE ${next.columnType}`);
		}
		if (prev.nullable !== next.nullable) {
			sql.push(
				next.nullable
					? `ALTER TABLE ${target} ALTER COLUMN ${col} DROP NOT NULL`
					: `ALTER TABLE ${target} ALTER COLUMN ${col} SET NOT NULL`
			);
		}
		if (prev.defaultValue !== next.defaultValue) {
			if (!next.defaultValue) {
				sql.push(`ALTER TABLE ${target} ALTER COLUMN ${col} DROP DEFAULT`);
			} else {
				sql.push(
					`ALTER TABLE ${target} ALTER COLUMN ${col} SET DEFAULT ${formatDefaultExpr(next.defaultValue)}`
				);
			}
		}
		if (prev.comment !== next.comment) {
			sql.push(commentSql(engine, schema, table, workingName, next.comment));
		}
		return sql;
	}

	if (isOracle(engine)) {
		const sql: string[] = [];
		let workingName = before.name;
		if (prev.name !== next.name) {
			sql.push(
				`ALTER TABLE ${target} RENAME COLUMN ${quoteIdent(engine, before.name)} TO ${quoteIdent(engine, next.name)}`
			);
			workingName = next.name;
		}
		const col = quoteIdent(engine, workingName);
		const changes: string[] = [col, next.columnType];
		changes.push(next.nullable ? 'NULL' : 'NOT NULL');
		if (prev.defaultValue !== next.defaultValue && next.defaultValue) {
			changes.push(`DEFAULT ${formatDefaultExpr(next.defaultValue)}`);
		}
		if (prev.columnType !== next.columnType || prev.nullable !== next.nullable || prev.defaultValue !== next.defaultValue) {
			sql.push(`ALTER TABLE ${target} MODIFY (${changes.join(' ')})`);
		}
		if (prev.comment !== next.comment) {
			sql.push(commentSql(engine, schema, table, workingName, next.comment));
		}
		return sql;
	}

	if (isMssql(engine)) {
		const sql: string[] = [];
		let workingName = before.name;
		const parts = mssqlParts(table);
		if (prev.name !== next.name) {
			sql.push(
				`EXEC sp_rename ${nLiteral(`${parts.schema}.${parts.name}.${before.name}`)}, ${nLiteral(next.name)}, ${nLiteral('COLUMN')}`
			);
			workingName = next.name;
		}
		if (prev.columnType !== next.columnType || prev.nullable !== next.nullable) {
			sql.push(
				`ALTER TABLE ${target} ALTER COLUMN ${quoteIdent(engine, workingName)} ${next.columnType} ${next.nullable ? 'NULL' : 'NOT NULL'}`
			);
		}
		if (prev.defaultValue !== next.defaultValue && next.defaultValue) {
			sql.push(
				`ALTER TABLE ${target} ADD CONSTRAINT ${quoteIdent(engine, `DF_${workingName}`)} DEFAULT ${formatDefaultExpr(next.defaultValue)} FOR ${quoteIdent(engine, workingName)}`
			);
		}
		if (prev.comment !== next.comment) {
			sql.push(mssqlColumnComment(table, workingName, next.comment, Boolean(prev.comment)));
		}
		return sql;
	}

	// MySQL: CHANGE when renaming, otherwise MODIFY. Full definition replaces the column.
	if (prev.name !== next.name) {
		return [
			`ALTER TABLE ${target} CHANGE COLUMN ${quoteIdent(engine, before.name)} ${mysqlColumnClause(next, before.extra)}`
		];
	}
	return [`ALTER TABLE ${target} MODIFY COLUMN ${mysqlColumnClause(next, before.extra)}`];
}

function mssqlColumnComment(table: string, column: string, comment: string, exists: boolean): string {
	const parts = mssqlParts(table);
	const proc = exists ? 'sys.sp_updateextendedproperty' : 'sys.sp_addextendedproperty';
	return `EXEC ${proc} @name=${nLiteral('MS_Description')}, @value=${nLiteral(comment.trim())}, @level0type=${nLiteral('SCHEMA')}, @level0name=${nLiteral(parts.schema)}, @level1type=${nLiteral('TABLE')}, @level1name=${nLiteral(parts.name)}, @level2type=${nLiteral('COLUMN')}, @level2name=${nLiteral(column)}`;
}

export async function runSqlStatements(
	execute: (sql: string) => Promise<unknown>,
	statements: string[]
): Promise<void> {
	for (const sql of statements) {
		if (!sql.trim()) continue;
		await execute(sql);
	}
}
