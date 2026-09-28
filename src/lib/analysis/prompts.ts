import { z } from 'zod';
import type { AnalysisLocale, CatalogObject, RelationshipEdge, SchemaCatalog } from './types';

export const COMMENT_TASK = 'TASK: comments';
export const RELATION_TASK = 'TASK: relations';

export const commentResponseSchema = z.object({
	comments: z
		.array(
			z.object({
				id: z.string(),
				comment: z.string(),
				confidence: z.enum(['high', 'medium', 'low'])
			})
		)
		.default([])
});

export const relationResponseSchema = z.object({
	relations: z
		.array(
			z.object({
				fromId: z.string(),
				toId: z.string(),
				fromColumns: z.array(z.string()).default([]),
				toColumns: z.array(z.string()).default([]),
				strength: z.enum(['strong', 'weak']),
				cardinality: z.enum(['one_to_one', 'one_to_many', 'many_to_one', 'many_to_many']),
				reason: z.string(),
				confidence: z.enum(['high', 'medium', 'low'])
			})
		)
		.default([])
});

export type CommentResponse = z.infer<typeof commentResponseSchema>;
export type RelationResponse = z.infer<typeof relationResponseSchema>;

export interface CommentItem {
	id: string;
	objectId: string;
	columnName?: string;
	prompt: string;
}

export function objectAliases(objects: CatalogObject[]): Map<string, string> {
	const aliases = new Map<string, string>();
	objects.forEach((object, index) => {
		aliases.set(object.id, `o${index + 1}`);
	});
	return aliases;
}

export function missingCommentItems(
	catalog: SchemaCatalog,
	aliases: Map<string, string>
): CommentItem[] {
	const items: CommentItem[] = [];
	for (const object of catalog.objects) {
		if (object.external) continue;
		const alias = aliases.get(object.id);
		if (!alias) continue;
		const columns = object.columns
			.map((column) => {
				const note = column.comment.trim() ? column.comment.trim() : 'MISSING COMMENT';
				return `${column.name} ${column.dataType} ${column.key} | ${note}`.trim();
			})
			.join('; ');
		if (!object.comment.trim()) {
			items.push({
				id: alias,
				objectId: object.id,
				prompt: `${alias} | ${object.schema}.${object.name} | ${object.kind} | columns: ${columns || '(none)'}`
			});
		}
		for (const column of object.columns) {
			if (column.comment.trim()) continue;
			items.push({
				id: `${alias}#${column.name}`,
				objectId: object.id,
				columnName: column.name,
				prompt: `${alias}#${column.name} | ${object.schema}.${object.name}.${column.name} | ${column.dataType} | siblings: ${columns || '(none)'}`
			});
		}
	}
	return items;
}

export function commentSystemPrompt(locale: AnalysisLocale): string {
	if (locale === 'zh') {
		return [
			'你是数据库结构分析员。只为列出的、缺少注释的表、视图和字段推断简洁业务含义。',
			'不要编造列表之外的对象。每条注释不超过 80 个字，使用简体中文。',
			'命名、类型和补充文档互相矛盾时，以文档为准，并保持谨慎。'
		].join('\n');
	}
	return [
		'You analyze database structure. Infer a short business meaning only for the listed tables, views, and columns that have no comment.',
		'Do not invent objects that are not listed. Keep each comment under 80 characters, in English.',
		'When names, types, and the supplementary documents disagree, prefer the documents and stay cautious.'
	].join('\n');
}

export function commentUserPrompt(items: CommentItem[], documents: string): string {
	return [
		COMMENT_TASK,
		'Write a comment for every id below.',
		'',
		items.map((item) => item.prompt).join('\n'),
		'',
		'Documents:',
		clip(documents, 8_000) || '(none)'
	].join('\n');
}

export function relationSystemPrompt(locale: AnalysisLocale): string {
	if (locale === 'zh') {
		return [
			'你是数据库关系分析员。结合表结构、注释、物理外键和补充文档，找出还没有声明为外键的关联。',
			'强关系：字段名、类型、注释和文档互相支持，例如 user_id 指向 users.id。',
			'弱关系：只有命名相似，或文档里的间接暗示。',
			'基数 many_to_one 表示多行 from 对应一行 to；one_to_many 相反；one_to_one 为一对一；many_to_many 为多对多。',
			'不要重复已经列出的物理外键。不要使用列表之外的对象 id。证据不足就不要输出。'
		].join('\n');
	}
	return [
		'You analyze database relationships. Using structure, comments, declared foreign keys, and supplementary documents, find associations that are not already foreign keys.',
		'Strong: names, types, comments, and documents agree, for example user_id pointing at users.id.',
		'Weak: only similar names, or an indirect hint in the documents.',
		'Cardinality many_to_one means many rows in from refer to one row in to; one_to_many is the reverse; one_to_one and many_to_many have the usual meanings.',
		'Do not repeat a physical foreign key that is already listed. Use only object ids from the list. Omit a relationship when the evidence is thin.'
	].join('\n');
}

export function relationUserPrompt(
	catalog: SchemaCatalog,
	aliases: Map<string, string>,
	known: RelationshipEdge[],
	documents: string
): string {
	const lines = catalog.objects.map((object) => {
		const alias = aliases.get(object.id) ?? object.id;
		const comment = object.inferredComment || object.comment || '(none)';
		const columns = object.columns
			.map((column) => {
				const note = column.inferredComment || column.comment;
				return `${column.name}:${column.dataType}${column.key ? ` ${column.key}` : ''}${note ? ` ${note}` : ''}`;
			})
			.join(', ');
		const marker = object.external ? ' external' : '';
		return `${alias} | ${object.schema}.${object.name} | ${object.kind}${marker} | ${comment} | ${columns}`;
	});
	const physical = known
		.filter((edge) => edge.origin === 'physical' && edge.reasonCode === 'foreign-key')
		.map((edge) => {
			const from = aliases.get(edge.fromId) ?? edge.fromId;
			const to = aliases.get(edge.toId) ?? edge.toId;
			return `${from}.${edge.fromColumns.join('+')} -> ${to}.${edge.toColumns.join('+')} (${edge.reason})`;
		});
	return [
		RELATION_TASK,
		'Objects:',
		lines.join('\n') || '(none)',
		'',
		'Declared foreign keys:',
		physical.join('\n') || '(none)',
		'',
		'Documents:',
		clip(documents, 12_000) || '(none)'
	].join('\n');
}

function clip(value: string, max: number): string {
	const text = value.trim();
	if (text.length <= max) return text;
	return `${text.slice(0, max)}\n[truncated]`;
}
