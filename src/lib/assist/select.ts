import type { CatalogObject } from '$lib/analysis/types';
import type { AssistCatalog } from './catalog';
import type { AssistContext } from './context';

export interface RelationHint {
	from: string;
	fromColumns: string[];
	to: string;
	toColumns: string[];
	origin: 'foreign-key' | 'inferred' | 'naming';
	confidence?: string;
}

export interface DraftScope {
	/** Tables and views whose columns are given to the model. */
	tables: CatalogObject[];
	/** Remaining object names, so the model can name candidates instead of guessing. */
	others: string[];
	relations: RelationHint[];
	/** Why each table was picked, keyed by table name. */
	reasons: Record<string, PickReason>;
}

export type PickReason = 'selected' | 'editor' | 'grid' | 'question' | 'related' | 'small-schema';

export const MAX_TABLES = 12;
export const MAX_OTHER_NAMES = 400;

const STOP_WORDS = new Set([
	'a',
	'an',
	'the',
	'of',
	'and',
	'or',
	'for',
	'in',
	'on',
	'by',
	'to',
	'per',
	'with',
	'from',
	'last',
	'each',
	'all',
	'show',
	'list',
	'get',
	'find',
	'how',
	'many',
	'what',
	'which',
	'who',
	'top',
	'count',
	'total',
	'sum',
	'avg',
	'id',
	'ids'
]);

function words(text: string): string[] {
	return text
		.replace(/([a-z])([A-Z])/g, '$1 $2')
		.toLowerCase()
		.split(/[^a-z0-9]+/)
		.filter((word) => word.length > 1 && !STOP_WORDS.has(word));
}

function stem(word: string): string {
	if (word.endsWith('ies') && word.length > 4) return `${word.slice(0, -3)}y`;
	if (word.endsWith('ses') || word.endsWith('xes')) return word.slice(0, -2);
	if (word.endsWith('s') && !word.endsWith('ss') && word.length > 3) return word.slice(0, -1);
	return word;
}

function cjkBigrams(text: string): Set<string> {
	const grams = new Set<string>();
	const runs = text.match(/[\p{Script=Han}]+/gu) ?? [];
	for (const run of runs) {
		for (let i = 0; i + 1 < run.length; i += 1) grams.add(run.slice(i, i + 2));
	}
	return grams;
}

function mentions(text: string, name: string): boolean {
	if (!text) return false;
	const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
	return new RegExp(`(^|[^\\p{L}\\p{N}_$])${escaped}($|[^\\p{L}\\p{N}_$])`, 'iu').test(text);
}

function describe(object: CatalogObject): string {
	return [object.comment, object.inferredComment ?? ''].join(' ');
}

function questionScore(object: CatalogObject, terms: Set<string>, grams: Set<string>): number {
	let score = 0;
	for (const word of words(object.name)) {
		if (terms.has(stem(word))) score += 6;
	}
	let columnHits = 0;
	for (const column of object.columns) {
		for (const word of words(column.name)) {
			if (terms.has(stem(word))) columnHits += 1;
		}
	}
	score += Math.min(columnHits, 3) * 2;
	if (grams.size) {
		let tableHits = 0;
		for (const gram of cjkBigrams(describe(object))) if (grams.has(gram)) tableHits += 1;
		score += Math.min(tableHits, 3) * 3;
		let columnGramHits = 0;
		for (const column of object.columns) {
			const text = `${column.comment} ${column.inferredComment ?? ''}`;
			for (const gram of cjkBigrams(text)) if (grams.has(gram)) columnGramHits += 1;
		}
		score += Math.min(columnGramHits, 4);
	}
	return score;
}

function nameOf(catalog: AssistCatalog, id: string): string | null {
	return catalog.objects.find((object) => object.id === id)?.name ?? null;
}

export function relationHints(catalog: AssistCatalog): RelationHint[] {
	const hints: RelationHint[] = [];
	for (const fk of catalog.foreignKeys) {
		const from = nameOf(catalog, fk.fromId);
		const to = nameOf(catalog, fk.toId);
		if (!from || !to) continue;
		hints.push({
			from,
			to,
			fromColumns: fk.pairs.map((pair) => pair.fromColumn),
			toColumns: fk.pairs.map((pair) => pair.toColumn),
			origin: 'foreign-key'
		});
	}
	for (const edge of catalog.inferred) {
		const from = nameOf(catalog, edge.fromId);
		const to = nameOf(catalog, edge.toId);
		if (!from || !to || !edge.fromColumns.length) continue;
		hints.push({
			from,
			to,
			fromColumns: edge.fromColumns,
			toColumns: edge.toColumns,
			origin: 'inferred',
			confidence: edge.confidence
		});
	}
	return [...hints, ...namingHints(catalog, hints)];
}

/**
 * `<table>_id` columns that point at another table's `id` but have neither a foreign key nor a
 * saved inferred relation. Undeclared joins like these are common, and without them the target
 * table is never pulled in as a neighbour.
 */
function namingHints(catalog: AssistCatalog, known: RelationHint[]): RelationHint[] {
	const covered = new Set(
		known.flatMap((hint) =>
			hint.fromColumns.map((column) => `${hint.from}.${column}`.toLowerCase())
		)
	);
	const byStem = new Map<string, CatalogObject>();
	for (const object of catalog.objects) {
		if (object.kind !== 'table' && object.kind !== 'view') continue;
		if (!object.columns.some((column) => column.name.toLowerCase() === 'id')) continue;
		byStem.set(stem(object.name.toLowerCase()), object);
	}
	const hints: RelationHint[] = [];
	for (const object of catalog.objects) {
		for (const column of object.columns) {
			const match = /^(.+)_id$/i.exec(column.name);
			if (!match || covered.has(`${object.name}.${column.name}`.toLowerCase())) continue;
			const target = byStem.get(stem(match[1].toLowerCase()));
			if (!target || target.name === object.name) continue;
			const id = target.columns.find((candidate) => candidate.name.toLowerCase() === 'id');
			hints.push({
				from: object.name,
				to: target.name,
				fromColumns: [column.name],
				toColumns: [id?.name ?? 'id'],
				origin: 'naming'
			});
		}
	}
	return hints;
}

/**
 * Pick the tables a question is most likely about: the selected object, tables already in the
 * editor or grid, name/comment matches with the question, then their join neighbours.
 */
export function pickTables(
	catalog: AssistCatalog,
	question: string,
	context: Pick<AssistContext, 'object' | 'editor' | 'grid' | 'error'>,
	limit = MAX_TABLES
): DraftScope {
	const allRelations = relationHints(catalog);
	const scores = new Map<string, number>();
	const reasons: Record<string, PickReason> = {};
	const bump = (name: string, score: number, reason: PickReason) => {
		if (score <= 0) return;
		const before = scores.get(name) ?? 0;
		scores.set(name, before + score);
		const ranks: PickReason[] = ['selected', 'grid', 'editor', 'question', 'related'];
		const current = reasons[name];
		if (!current || ranks.indexOf(reason) < ranks.indexOf(current)) reasons[name] = reason;
	};

	const terms = new Set(words(question).map(stem));
	const grams = cjkBigrams(question);
	const editorText = [context.editor?.selection || context.editor?.text || '', context.error ?? '']
		.join('\n')
		.slice(0, 20_000);
	const gridSql = context.grid?.sql ?? '';

	for (const object of catalog.objects) {
		const lower = object.name.toLowerCase();
		if (context.object && context.object.name.toLowerCase() === lower) {
			bump(object.name, 100, 'selected');
		}
		if (context.grid?.object && context.grid.object.name.toLowerCase() === lower) {
			bump(object.name, 60, 'grid');
		} else if (mentions(gridSql, object.name)) {
			bump(object.name, 40, 'grid');
		}
		if (mentions(editorText, object.name)) bump(object.name, 50, 'editor');
		if (mentions(question, object.name)) bump(object.name, 30, 'question');
		bump(object.name, questionScore(object, terms, grams), 'question');
	}

	const seeds = [...scores.entries()].sort((a, b) => b[1] - a[1]).slice(0, limit);
	for (const [name, score] of seeds) {
		for (const hint of allRelations) {
			const other = hint.from === name ? hint.to : hint.to === name ? hint.from : null;
			if (other && other !== name) bump(other, Math.max(1, score * 0.3), 'related');
		}
	}

	const ranked = [...scores.entries()].sort((a, b) => b[1] - a[1]).map(([name]) => name);
	if (catalog.objects.length <= limit) {
		for (const object of catalog.objects) {
			if (scores.has(object.name)) continue;
			ranked.push(object.name);
			reasons[object.name] = 'small-schema';
		}
	}
	const pickedNames = new Set(ranked.slice(0, limit));
	const tables = catalog.objects.filter((object) => pickedNames.has(object.name));
	tables.sort((a, b) => ranked.indexOf(a.name) - ranked.indexOf(b.name));
	const others = catalog.objects
		.filter((object) => !pickedNames.has(object.name))
		.map((object) => object.name)
		.slice(0, MAX_OTHER_NAMES);
	const relations = allRelations.filter(
		(hint) => pickedNames.has(hint.from) && pickedNames.has(hint.to)
	);
	const pickedReasons: Record<string, PickReason> = {};
	for (const name of pickedNames) pickedReasons[name] = reasons[name] ?? 'related';
	return { tables, others, relations, reasons: pickedReasons };
}
