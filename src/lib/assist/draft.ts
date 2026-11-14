import type { StructuredCaller } from '$lib/analysis/workflow';
import type { SchemaScope } from '$lib/analysis/types';
import { normalizeEngine } from '$lib/engine';
import type { AssistCatalog } from './catalog';
import { describeContext, type AssistContext } from './context';
import {
	draftHumanPrompt,
	draftResponseSchema,
	draftSystemPrompt,
	formatScope,
	type DraftResponse
} from './prompt';
import { pickTables, type DraftScope } from './select';
import { validateDraft, type DraftIssue } from './validate';

export type DraftPhase = 'catalog' | 'drafting' | 'checking' | 'repairing';

export type DraftOutcome =
	| {
			kind: 'draft';
			response: DraftResponse;
			sql: string;
			scope: DraftScope;
			catalogCached: boolean;
			repaired: boolean;
	  }
	| {
			kind: 'clarify';
			response: DraftResponse;
			scope: DraftScope;
			catalogCached: boolean;
	  }
	| {
			/** The model kept referencing objects that are not in the catalog. Nothing is inserted. */
			kind: 'rejected';
			response: DraftResponse;
			sql: string;
			issues: DraftIssue[];
			scope: DraftScope;
			catalogCached: boolean;
	  }
	| { kind: 'empty-catalog'; schema: string };

export class DraftError extends Error {
	constructor(
		readonly stage: 'catalog' | 'model',
		message: string
	) {
		super(message);
	}
}

export class DraftCancelled extends Error {
	constructor() {
		super('cancelled');
	}
}

export interface DraftInput {
	question: string;
	context: AssistContext;
	caller: StructuredCaller;
	locale: 'en' | 'zh';
	oracleVersion?: string | null;
	/** Earlier questions and drafts in this editor, newest last. */
	followUp?: string;
	loadCatalog: (scope: SchemaScope) => Promise<{ catalog: AssistCatalog; cached: boolean }>;
	onPhase?: (phase: DraftPhase) => void;
	isCancelled?: () => boolean;
}

/** Strip Markdown fences and, for Oracle, the trailing semicolon its drivers reject. */
export function cleanDraftSql(sql: string, engine: string): string {
	let text = sql.trim();
	const fenced = /^```[a-z]*\s*\n?([\s\S]*?)\n?```$/i.exec(text);
	if (fenced) text = fenced[1].trim();
	if (normalizeEngine(engine) === 'oracle') text = text.replace(/;\s*$/, '');
	return text;
}

function feedbackFor(sql: string, issues: DraftIssue[]): string {
	const names = issues.map((issue) => `${issue.kind} ${issue.name}`).join(', ');
	return [
		'Your previous draft was:',
		sql,
		`It references objects that do not exist in the catalog: ${names}.`,
		'Rewrite it using only the tables and columns listed above, or return status "clarify" if the catalog does not have what the question needs.'
	].join('\n');
}

export async function draftSql(input: DraftInput): Promise<DraftOutcome> {
	const { context } = input;
	const schema = context.schema;
	if (!schema) throw new DraftError('catalog', 'no schema');
	const checkCancelled = () => {
		if (input.isCancelled?.()) throw new DraftCancelled();
	};

	input.onPhase?.('catalog');
	let loaded: { catalog: AssistCatalog; cached: boolean };
	try {
		loaded = await input.loadCatalog({
			connectionId: context.connectionId,
			connectionName: context.connectionName,
			engine: context.engine,
			schema,
			oracleVersion: input.oracleVersion
		});
	} catch (error) {
		throw new DraftError('catalog', error instanceof Error ? error.message : String(error));
	}
	checkCancelled();
	const { catalog, cached } = loaded;
	if (!catalog.objects.length) return { kind: 'empty-catalog', schema };

	const scope = pickTables(catalog, input.question, context);
	const system = draftSystemPrompt(context.engine, input.locale);
	const catalogText = formatScope(schema, scope, catalog.indexes);
	const contextText = describeContext(context);
	const ask = async (feedback?: string) => {
		try {
			return await input.caller.complete(
				draftResponseSchema,
				system,
				draftHumanPrompt({
					question: input.question,
					context: contextText,
					catalog: catalogText,
					followUp: input.followUp,
					feedback
				})
			);
		} catch (error) {
			throw new DraftError('model', error instanceof Error ? error.message : String(error));
		}
	};

	input.onPhase?.('drafting');
	let response = await ask();
	checkCancelled();
	if (response.status === 'clarify' || !response.sql.trim()) {
		return { kind: 'clarify', response, scope, catalogCached: cached };
	}

	input.onPhase?.('checking');
	let sql = cleanDraftSql(response.sql, context.engine);
	let issues = validateDraft(sql, catalog.objects, schema, response.columns);
	if (!issues.length) {
		return { kind: 'draft', response, sql, scope, catalogCached: cached, repaired: false };
	}

	input.onPhase?.('repairing');
	response = await ask(feedbackFor(sql, issues));
	checkCancelled();
	if (response.status === 'clarify' || !response.sql.trim()) {
		return { kind: 'clarify', response, scope, catalogCached: cached };
	}
	sql = cleanDraftSql(response.sql, context.engine);
	issues = validateDraft(sql, catalog.objects, schema, response.columns);
	if (issues.length) {
		return { kind: 'rejected', response, sql, issues, scope, catalogCached: cached };
	}
	return { kind: 'draft', response, sql, scope, catalogCached: cached, repaired: true };
}
