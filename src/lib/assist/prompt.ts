import { z } from 'zod';
import type { CatalogObject } from '$lib/analysis/types';
import type { IndexInfo } from '$lib/api/types';
import { engineLabel, normalizeEngine } from '$lib/engine';
import { maskSecrets } from './context';
import type { DraftScope } from './select';

export const draftResponseSchema = z.object({
	status: z
		.enum(['draft', 'clarify'])
		.describe('draft: SQL is ready. clarify: the catalog is not enough to write it safely.'),
	sql: z.string().describe('One SQL statement, empty when status is clarify'),
	summary: z.string().describe('One or two sentences on what the SQL does or what is missing'),
	tables: z.array(z.string()).describe('Tables and views used, as listed in the catalog'),
	columns: z.array(z.string()).describe('Columns used, as table.column'),
	relations: z
		.array(z.string())
		.describe('Join conditions used, e.g. orders.customer_id = customers.id'),
	assumptions: z.array(z.string()).describe('Assumptions the user should check'),
	candidates: z.array(z.string()).describe('When clarifying: table names that might be meant'),
	missing: z.string().describe('When clarifying: what information is missing')
});

export type DraftResponse = z.infer<typeof draftResponseSchema>;

const MAX_COLUMNS_PER_TABLE = 80;

function dialectRules(engine: string): string[] {
	switch (normalizeEngine(engine)) {
		case 'postgres':
			return [
				'Quote identifiers with double quotes only when they contain upper-case letters or special characters.',
				'Limit rows with LIMIT n. Use date_trunc, now(), interval literals and ILIKE where useful.'
			];
		case 'mssql':
			return [
				'Quote identifiers with [brackets] when needed. Prefix tables with their schema, e.g. dbo.orders.',
				'Limit rows with SELECT TOP (n), or OFFSET … FETCH NEXT n ROWS ONLY after ORDER BY. Use DATEADD, GETDATE(), CAST(x AS date).'
			];
		case 'oracle':
			return [
				'Identifiers are upper-case unless created quoted; quote with double quotes only when the catalog name is not upper-case.',
				'Limit rows with FETCH FIRST n ROWS ONLY. Use SYSDATE, TRUNC, INTERVAL and TO_CHAR. Do not end the statement with a semicolon.'
			];
		default:
			return [
				'Quote identifiers with backticks when needed.',
				'Limit rows with LIMIT n. Use NOW(), DATE_SUB, DATE_FORMAT and INTERVAL n DAY.'
			];
	}
}

export function draftSystemPrompt(engine: string, locale: 'en' | 'zh'): string {
	const language = locale === 'zh' ? 'Simplified Chinese' : 'English';
	return [
		`You draft SQL for a ${engineLabel(engine)} database inside a desktop database client.`,
		'The user reviews and runs your SQL themselves. You never execute anything.',
		'',
		'Rules:',
		'- Use only tables, views and columns listed under "Relevant tables". Never invent a table or column.',
		'- If the question needs a table that is only listed under "Other tables", or no listed table fits, return status "clarify" with candidates and what is missing instead of guessing.',
		'- Write exactly one statement. Prefer a read-only SELECT. Write INSERT/UPDATE/DELETE or DDL only when the user explicitly asks to change data or structure.',
		'- Join on the listed relationships. Foreign keys are reliable; inferred relations are guesses, so mention them in assumptions when you use them.',
		'- Join only tables the result needs. When summing or counting a parent row, do not join a one-to-many child that is not aggregated first, or the parent values are counted once per child row; pre-aggregate the child in a subquery instead.',
		'- Give every table an alias and qualify every column with it, e.g. o.amount.',
		'- Add a row limit of 100 to queries that return detail rows; aggregates need none.',
		'- Tables in the current schema may be used without a schema prefix unless the dialect rules say otherwise.',
		'- When the editor already has SQL and the question asks to change it, rewrite that SQL instead of starting over.',
		'- Do not put explanations inside the SQL.',
		...dialectRules(engine).map((rule) => `- ${rule}`),
		'',
		`Write summary, assumptions and missing in ${language}. Keep SQL keywords in upper case.`
	].join('\n');
}

function formatColumns(object: CatalogObject): string {
	const columns = object.columns.slice(0, MAX_COLUMNS_PER_TABLE).map((column) => {
		const key = /pri/i.test(column.key) ? ' PK' : '';
		const comment = column.comment || column.inferredComment || '';
		return `  ${column.name} ${column.dataType}${key}${comment ? ` -- ${comment.replace(/\s+/g, ' ').slice(0, 120)}` : ''}`;
	});
	if (object.columns.length > MAX_COLUMNS_PER_TABLE) {
		columns.push(`  … ${object.columns.length - MAX_COLUMNS_PER_TABLE} more columns`);
	}
	return columns.join('\n');
}

export function formatScope(schema: string, scope: DraftScope, indexes: IndexInfo[]): string {
	const lines: string[] = [];
	lines.push(`Relevant tables (schema ${schema}):`);
	if (!scope.tables.length) lines.push('(none matched the question)');
	for (const object of scope.tables) {
		const comment = object.comment || object.inferredComment || '';
		lines.push(
			`${object.name} (${object.kind})${comment ? ` -- ${comment.replace(/\s+/g, ' ')}` : ''}`
		);
		lines.push(formatColumns(object));
		const tableIndexes = indexes.filter(
			(index) => index.tableName.toLowerCase() === object.name.toLowerCase() && !index.primary
		);
		if (tableIndexes.length) {
			lines.push(
				`  indexes: ${tableIndexes.map((index) => `(${index.columns.join(', ')})${index.unique ? ' unique' : ''}`).join('; ')}`
			);
		}
	}
	if (scope.relations.length) {
		lines.push('', 'Relationships:');
		for (const hint of scope.relations) {
			const pairs = hint.fromColumns
				.map((column, i) => `${hint.from}.${column} = ${hint.to}.${hint.toColumns[i] ?? '?'}`)
				.join(' AND ');
			const origin =
				hint.origin === 'foreign-key'
					? 'foreign key'
					: hint.origin === 'naming'
						? 'column name only, no constraint'
						: `inferred, ${hint.confidence ?? 'unknown'} confidence`;
			lines.push(`${pairs} (${origin})`);
		}
	}
	if (scope.others.length) {
		lines.push('', `Other tables (names only): ${scope.others.join(', ')}`);
	}
	return lines.join('\n');
}

export function draftHumanPrompt(input: {
	question: string;
	context: string;
	catalog: string;
	followUp?: string;
	pinned?: string[];
	feedback?: string;
}): string {
	const parts = [
		`Question: ${maskSecrets(input.question)}`,
		'',
		'Workspace:',
		input.context,
		'',
		input.catalog
	];
	if (input.pinned?.length) {
		parts.push(
			'',
			`The user confirmed the question is about these tables: ${input.pinned.join(', ')}. Read the question in terms of them (for example, which amount is meant) and draft now unless something essential is still missing.`
		);
	}
	if (input.followUp) parts.push('', 'Earlier in this session:', maskSecrets(input.followUp));
	if (input.feedback) parts.push('', input.feedback);
	return parts.join('\n');
}
