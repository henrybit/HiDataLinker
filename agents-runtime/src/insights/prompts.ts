import { z } from 'zod';
import type { AgentLocale } from '../data-analyst/prompts.js';

export const INSIGHT_SEVERITIES = ['high', 'medium', 'low'] as const;
export const INSIGHT_CATEGORIES = [
	'business',
	'quality',
	'performance',
	'governance',
	'integration'
] as const;

export const profileResponseSchema = z.object({
	connections: z
		.array(
			z.object({
				connectionId: z.string(),
				domain: z.string(),
				summary: z.string(),
				keyTables: z
					.array(
						z.object({
							schema: z.string(),
							table: z.string()
						})
					)
					.max(6)
			})
		)
		.max(12)
});

export const recommendResponseSchema = z.object({
	insights: z
		.array(
			z.object({
				title: z.string(),
				body: z.string(),
				severity: z.enum(INSIGHT_SEVERITIES),
				category: z.enum(INSIGHT_CATEGORIES),
				connectionIds: z.array(z.string()),
				tables: z.array(z.string()),
				tags: z.array(z.string()),
				prompt: z.string()
			})
		)
		.min(1)
		.max(10)
});

export type ConnectionProfileResult = z.infer<typeof profileResponseSchema>['connections'][number];

function outputLanguage(locale: AgentLocale): string {
	return locale === 'zh'
		? '所有自然语言字段（domain、summary、title、body、tags、prompt）必须使用简体中文；表名、字段名、连接 id 保持原样。'
		: 'Write every natural-language field (domain, summary, title, body, tags, prompt) in English; keep table/column names and connection ids unchanged.';
}

export function profileSystemPrompt(locale: AgentLocale): string {
	return [
		'You are a database metadata analyst embedded in a desktop database client.',
		'Given a digest of the saved connections and their schemas/tables, infer the business domain of each connection',
		'(e.g. billing, e-commerce orders, CRM, inventory, logging) and pick the tables that best represent its core entities and facts.',
		'Only reference connection ids, schemas and tables that literally appear in the digest. Never invent names.',
		'Skip connections whose metadata is missing or failed.',
		outputLanguage(locale)
	].join('\n');
}

export function profileUserPrompt(schemaDigest: string): string {
	return [
		'Connection metadata digest:',
		schemaDigest,
		'',
		'Return, per connection: connectionId, domain (short label), summary (one sentence), and up to 6 keyTables {schema, table}.'
	].join('\n');
}

export function recommendSystemPrompt(locale: AgentLocale): string {
	return [
		'You are the Insights agent of a database GUI. Your job is to recommend concrete, high-value analysis scenarios',
		'that the user can run right now against their own databases, based only on the metadata the app has collected',
		'(connections, schemas, tables, columns, types, keys). You have not seen any row data.',
		'',
		'Good recommendations:',
		'- are grounded in specific tables/columns that exist in the metadata;',
		'- cover a mix of categories: business (KPIs, trends, funnels, cohorts), quality (nulls, duplicates, orphan foreign keys),',
		'  performance (missing indexes, wide tables, large fact tables), governance (sensitive columns such as phone/email/id numbers,',
		'  missing primary keys, inconsistent naming), integration (cross-connection joins on shared keys);',
		'- explain why it matters in 1-3 sentences (body);',
		'- include a ready-to-send prompt for the data-analyst agent that names the connection, schema and tables explicitly;',
		'- use severity "high" for likely risks or high business value, "medium" for useful checks, "low" for nice-to-have.',
		'',
		'Do not claim facts about the data values (you have not queried rows); phrase findings as hypotheses to verify.',
		'connectionIds must be ids from the metadata. tables use "schema.table" form. tags are 1-4 short labels (columns, metrics).',
		outputLanguage(locale)
	].join('\n');
}

export function recommendUserPrompt(input: {
	schemaDigest: string;
	profiles: ConnectionProfileResult[];
	columnNotes: string[];
	maxInsights: number;
}): string {
	const profileLines = input.profiles.map(
		(item) =>
			`- ${item.connectionId}: ${item.domain} — ${item.summary} (key tables: ${
				item.keyTables.map((t) => `${t.schema}.${t.table}`).join(', ') || 'n/a'
			})`
	);
	return [
		'Connection metadata digest:',
		input.schemaDigest,
		'',
		'Inferred connection profiles:',
		profileLines.join('\n') || '(none)',
		'',
		'Column details of key tables:',
		input.columnNotes.join('\n') || '(unavailable)',
		'',
		`Recommend between 3 and ${input.maxInsights} scenarios, most valuable first.`
	].join('\n');
}

export function insightsStatus(
	locale: AgentLocale,
	phase: 'collect' | 'profile' | 'inspect' | 'recommend' | 'finalize'
): string {
	const zh: Record<typeof phase, string> = {
		collect: '正在汇总连接元数据…',
		profile: '正在识别各连接的业务领域…',
		inspect: '正在读取关键表的字段结构…',
		recommend: '正在生成推荐场景…',
		finalize: '正在整理洞察结果…'
	};
	const en: Record<typeof phase, string> = {
		collect: 'Collecting connection metadata…',
		profile: 'Identifying business domains…',
		inspect: 'Reading key table columns…',
		recommend: 'Generating recommended scenarios…',
		finalize: 'Finalizing insights…'
	};
	return (locale === 'zh' ? zh : en)[phase];
}
