import type { QueryResult } from '$lib/api/types';

/** Where the user asked from. */
export type AssistOrigin = 'editor' | 'tree' | 'grid' | 'table' | 'error';

export interface AssistObjectRef {
	schema: string;
	name: string;
	kind: 'table' | 'view';
}

export interface AssistGridContext {
	/** Statement that produced the grid. */
	sql: string;
	columns: string[];
	rowCount: number;
	truncated: boolean;
	object: AssistObjectRef | null;
}

/**
 * Workspace facts a draft request may use. Built from live UI state at submit time, so a
 * changed connection, schema or tree selection is picked up by the next question.
 * Deliberately has no host, port, user name, password or connection string.
 */
export interface AssistContext {
	origin: AssistOrigin;
	connectionId: string;
	connectionName: string;
	engine: string;
	connected: boolean;
	schema: string | null;
	object: AssistObjectRef | null;
	editor: { text: string; selection: string | null } | null;
	grid: AssistGridContext | null;
	error: string | null;
}

/** How a query tab was opened from another entry point. */
export interface AssistSeed {
	origin: AssistOrigin;
	object?: AssistObjectRef | null;
}

export type AssistBlocker = 'no-connection' | 'not-connected' | 'no-schema' | 'no-model';

/** First reason a question cannot be sent yet; `null` when the request may go out. */
export function assistBlocker(
	context: Pick<AssistContext, 'connectionId' | 'connected' | 'schema'> | null,
	hasModel: boolean
): AssistBlocker | null {
	if (!context?.connectionId) return 'no-connection';
	if (!context.connected) return 'not-connected';
	if (!context.schema) return 'no-schema';
	if (!hasModel) return 'no-model';
	return null;
}

const MAX_EDITOR_CHARS = 8_000;
const MAX_ERROR_CHARS = 2_000;
const MAX_GRID_COLUMNS = 80;

const SECRET_PATTERNS: Array<[RegExp, string]> = [
	[/\b(IDENTIFIED\s+BY\s+)('[^']*'|"[^"]*"|\S+)/gi, '$1***'],
	[/\b(PASSWORD\s*(?:=|\s)\s*)('[^']*'|"[^"]*")/gi, '$1***'],
	[/\b((?:password|passwd|pwd|secret|token|api[_-]?key)\s*[=:]\s*)([^\s;,'"]+)/gi, '$1***'],
	[/([a-z][a-z0-9+.-]*:\/\/[^\s:/@]+:)[^\s@/]+@/gi, '$1***@']
];

/** Redact inline credentials (CREATE USER … IDENTIFIED BY, URLs with passwords, key=value). */
export function maskSecrets(text: string): string {
	let masked = text;
	for (const [pattern, replacement] of SECRET_PATTERNS) {
		masked = masked.replace(pattern, replacement);
	}
	return masked;
}

function clip(text: string, max: number): string {
	return text.length > max ? `${text.slice(0, max)}\n-- … (${text.length - max} more chars)` : text;
}

function unquote(identifier: string): string {
	const trimmed = identifier.trim();
	const first = trimmed[0];
	const last = trimmed[trimmed.length - 1];
	if (
		(first === '"' && last === '"') ||
		(first === '`' && last === '`') ||
		(first === '[' && last === ']')
	) {
		return trimmed.slice(1, -1);
	}
	return trimmed;
}

const IDENT = String.raw`(?:"[^"]+"|\x60[^\x60]+\x60|\[[^\]]+\]|[\p{L}_][\p{L}\p{N}_$#]*)`;
const SINGLE_FROM = new RegExp(
	String.raw`^\s*(?:select|with)\b[\s\S]*?\bfrom\s+(${IDENT})(?:\s*\.\s*(${IDENT}))?(?:\s|;|$)`,
	'iu'
);

/** Object a single-table SELECT reads from, for describing what a grid shows. */
export function resultObject(sql: string, defaultSchema: string | null): AssistObjectRef | null {
	const text = sql.replace(/--[^\n]*|\/\*[\s\S]*?\*\//g, ' ');
	if (/\bjoin\b/i.test(text)) return null;
	const match = SINGLE_FROM.exec(text);
	if (!match) return null;
	const [, first, second] = match;
	if (second) return { schema: unquote(first), name: unquote(second), kind: 'table' };
	if (!defaultSchema) return null;
	return { schema: defaultSchema, name: unquote(first), kind: 'table' };
}

export function gridContext(
	sql: string,
	result: QueryResult | null,
	defaultSchema: string | null
): AssistGridContext | null {
	if (!result || !result.columns.length) return null;
	return {
		sql,
		columns: result.columns.map((column) => column.name),
		rowCount: result.rows.length,
		truncated: result.truncated,
		object: resultObject(sql, defaultSchema)
	};
}

/**
 * Plain-text description of the workspace for the model. Contains no connection name,
 * host or credentials, and never any result rows.
 */
export function describeContext(context: AssistContext): string {
	const lines = [`Engine: ${context.engine}`];
	if (context.schema) lines.push(`Current schema/database: ${context.schema}`);
	if (context.object) {
		lines.push(`Selected ${context.object.kind}: ${context.object.schema}.${context.object.name}`);
	}
	if (context.editor?.selection?.trim()) {
		lines.push(
			'Selected text in the SQL editor:',
			clip(maskSecrets(context.editor.selection), MAX_EDITOR_CHARS)
		);
	} else if (context.editor?.text.trim()) {
		lines.push(
			'Current SQL editor content:',
			clip(maskSecrets(context.editor.text), MAX_EDITOR_CHARS)
		);
	}
	if (context.grid) {
		const columns = context.grid.columns.slice(0, MAX_GRID_COLUMNS);
		lines.push(
			`The result grid shows ${context.grid.rowCount}${context.grid.truncated ? '+' : ''} rows with columns: ${columns.join(', ')}${context.grid.columns.length > columns.length ? ', …' : ''}`
		);
		if (context.grid.object) {
			lines.push(
				`The grid result comes from ${context.grid.object.schema}.${context.grid.object.name}`
			);
		}
		if (context.grid.sql.trim() && context.grid.sql !== context.editor?.text) {
			lines.push(
				'Statement that produced the grid:',
				clip(maskSecrets(context.grid.sql), MAX_EDITOR_CHARS)
			);
		}
	}
	if (context.error?.trim()) {
		lines.push('The last run failed with:', clip(maskSecrets(context.error), MAX_ERROR_CHARS));
	}
	return lines.join('\n');
}
