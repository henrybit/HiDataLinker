import type { CatalogObject } from '$lib/analysis/types';

export interface DraftIssue {
	kind: 'table' | 'column';
	/** `table` or `table.column` as written in the draft. */
	name: string;
}

const IDENT = String.raw`(?:"[^"]+"|\x60[^\x60]+\x60|\[[^\]]+\]|[\p{L}_][\p{L}\p{N}_$#]*)`;
const QUALIFIED = String.raw`${IDENT}(?:\s*\.\s*${IDENT}){0,2}`;
const TABLE_REF = new RegExp(
	String.raw`\b(?:from|join|update|into)\s+(${QUALIFIED})(?:\s+(?:as\s+)?(${IDENT}))?`,
	'giu'
);
const COLUMN_REF = new RegExp(String.raw`(${IDENT})\s*\.\s*(${IDENT}|\*)`, 'gu');
const CTE_NAME = new RegExp(
	String.raw`(?:\bwith(?:\s+recursive)?|,)\s+(${IDENT})\s+as\s*\(`,
	'giu'
);

const NOT_ALIASES = new Set([
	'on',
	'where',
	'join',
	'left',
	'right',
	'inner',
	'outer',
	'cross',
	'full',
	'group',
	'order',
	'limit',
	'using',
	'natural',
	'union',
	'intersect',
	'except',
	'minus',
	'set',
	'values',
	'select',
	'having',
	'window',
	'fetch',
	'offset',
	'lateral',
	'as',
	'default',
	'returning',
	'with',
	'top',
	'pivot',
	'unpivot',
	'tablesample',
	'for'
]);

/** Schemas whose objects are not in the user's catalog read and are not checked. */
const SYSTEM_SCHEMAS = new Set([
	'information_schema',
	'pg_catalog',
	'sys',
	'mysql',
	'performance_schema'
]);

function unquote(identifier: string): string {
	const trimmed = identifier.trim();
	const first = trimmed[0];
	if (first === '"' || first === '`' || first === '[') return trimmed.slice(1, -1);
	return trimmed;
}

/** Position of the unclosed `(` around `index`, or -1. */
function enclosingParen(text: string, index: number): number {
	let depth = 0;
	for (let i = index - 1; i >= 0; i -= 1) {
		const char = text[i];
		if (char === ')') depth += 1;
		else if (char === '(') {
			if (depth === 0) return i;
			depth -= 1;
		}
	}
	return -1;
}

/** `FROM` inside EXTRACT(… FROM x), SUBSTRING(… FROM …) or TRIM(… FROM …), not a table. */
function inFunctionArguments(text: string, index: number): boolean {
	const open = enclosingParen(text, index);
	return open >= 0 && !/\bselect\b/i.test(text.slice(open, index));
}

/** Comments and string literals removed so their contents are not mistaken for identifiers. */
export function stripSql(sql: string): string {
	return sql
		.replace(/--[^\n]*/g, ' ')
		.replace(/\/\*[\s\S]*?\*\//g, ' ')
		.replace(/'(?:[^']|'')*'/g, "''");
}

/**
 * Tables and alias-qualified columns in `sql` that do not exist in `objects` (the current
 * schema's catalog). References into other schemas, CTEs, subqueries and table functions are
 * not checked.
 */
export function validateDraft(
	sql: string,
	objects: CatalogObject[],
	schema: string,
	declaredColumns: string[] = []
): DraftIssue[] {
	const text = stripSql(sql);
	const byName = new Map(objects.map((object) => [object.name.toLowerCase(), object]));
	const ctes = new Set<string>();
	for (const match of text.matchAll(CTE_NAME)) ctes.add(unquote(match[1]).toLowerCase());

	const issues: DraftIssue[] = [];
	const seen = new Set<string>();
	const report = (issue: DraftIssue) => {
		const key = `${issue.kind}:${issue.name.toLowerCase()}`;
		if (seen.has(key)) return;
		seen.add(key);
		issues.push(issue);
	};

	const refs = [...text.matchAll(TABLE_REF)]
		.filter((match) => !inFunctionArguments(text, match.index ?? 0))
		.map((match) => {
			const parts = match[1].split(/\s*\.\s*/).map(unquote);
			const alias = match[2] ? unquote(match[2]) : null;
			return {
				name: parts[parts.length - 1],
				owner: parts.length > 1 ? parts[parts.length - 2] : null,
				aliasKey: alias && !NOT_ALIASES.has(alias.toLowerCase()) ? alias.toLowerCase() : null,
				call: text
					.slice((match.index ?? 0) + match[1].length + match[0].indexOf(match[1]))
					.startsWith('(')
			};
		});
	const aliasNames = new Set(refs.flatMap((ref) => (ref.aliasKey ? [ref.aliasKey] : [])));

	const aliases = new Map<string, CatalogObject | null>();
	const schemaNames = new Set<string>();
	for (const { name, owner, aliasKey, call } of refs) {
		if (owner && aliasNames.has(owner.toLowerCase())) continue;
		if (owner) schemaNames.add(owner.toLowerCase());
		const lower = name.toLowerCase();
		if (call || NOT_ALIASES.has(lower)) continue;
		if (ctes.has(lower) || (lower === 'dual' && !owner)) continue;
		const foreign =
			(owner && owner.toLowerCase() !== schema.toLowerCase()) ||
			(owner && SYSTEM_SCHEMAS.has(owner.toLowerCase()));
		const object = foreign ? null : (byName.get(lower) ?? null);
		if (!foreign && !object) report({ kind: 'table', name });
		aliases.set(lower, object);
		if (aliasKey) aliases.set(aliasKey, object);
	}

	const checkColumn = (owner: string, column: string) => {
		const object = aliases.get(owner.toLowerCase()) ?? byName.get(owner.toLowerCase());
		if (!object || column === '*') return;
		if (!object.columns.some((item) => item.name.toLowerCase() === column.toLowerCase())) {
			report({ kind: 'column', name: `${object.name}.${column}` });
		}
	};

	for (const match of text.matchAll(COLUMN_REF)) {
		const owner = unquote(match[1]);
		const column = unquote(match[2]);
		const ownerLower = owner.toLowerCase();
		if (schemaNames.has(ownerLower) || ownerLower === schema.toLowerCase()) continue;
		if (!aliases.has(ownerLower)) continue;
		const following = text.slice((match.index ?? 0) + match[0].length).trimStart();
		if (following.startsWith('(') || following.startsWith('.')) continue;
		checkColumn(owner, column);
	}

	for (const declared of declaredColumns) {
		const parts = declared.split('.').map((part) => unquote(part));
		if (parts.length < 2) continue;
		const table = parts[parts.length - 2];
		const column = parts[parts.length - 1];
		if (!byName.has(table.toLowerCase())) continue;
		checkColumn(table, column);
	}

	return issues;
}
