import { z } from 'zod';
import type { AnalystObservation, AnalystPlanStep } from './events.js';

export type AgentLocale = 'zh' | 'en';

export function normalizeLocale(value: unknown): AgentLocale {
	return value === 'zh' ? 'zh' : 'en';
}

export function languageDirective(locale: AgentLocale): string {
	if (locale === 'zh') {
		return [
			'用户界面语言为简体中文。',
			'计划步骤的 title/goal、执行备注 note、最终 Markdown 回答必须使用简体中文。',
			'SQL、表名、字段名、连接 id 等标识符保持原样，不要翻译。'
		].join('\n');
	}
	return [
		'The UI language is English.',
		'Write plan step titles/goals, execution notes, and the final Markdown answer in English.',
		'Keep SQL and identifiers (table/column/connection ids) unchanged; do not translate them.'
	].join('\n');
}

export const planResponseSchema = z.object({
	steps: z
		.array(
			z.object({
				title: z.string(),
				goal: z.string(),
				connectionHint: z.string().optional(),
				tablesHint: z.array(z.string()).optional(),
				toolHint: z.string().optional(),
				sqlIntent: z.string().optional()
			})
		)
		.min(1)
		.max(8)
});

export const actResponseSchema = z.object({
	done: z.boolean(),
	note: z.string(),
	tool: z
		.enum([
			'list_connections',
			'connect',
			'list_databases',
			'list_tables',
			'list_views',
			'get_columns',
			'get_ddl',
			'preview_table',
			'query',
			'none'
		])
		.optional(),
	args: z.record(z.string(), z.unknown()).optional()
});

export const synthesizeResponseSchema = z.object({
	markdown: z.string().min(1)
});

export function planSystemPrompt(systemMd: string, locale: AgentLocale): string {
	if (locale === 'zh') {
		return `${systemMd}

${languageDirective(locale)}

你负责设计简短的多步分析计划。每一步要具体、且可借助工具完成。
优先先探查 schema，再写 SQL。最多 8 步。`;
	}
	return `${systemMd}

${languageDirective(locale)}

You design a short multi-step analysis plan. Each step should be concrete and tool-backed.
Prefer discovering schema before writing SQL. Cap at 8 steps.`;
}

export function planUserPrompt(query: string, schemaDigest: string, locale: AgentLocale): string {
	if (locale === 'zh') {
		return `用户问题：
${query}

Schema 摘要（连接 / 库 / 样例表）：
${schemaDigest}

只返回 JSON steps。`;
	}
	return `User question:
${query}

Schema digest (connections / databases / sample tables):
${schemaDigest}

Return JSON steps only.`;
}

export function actSystemPrompt(systemMd: string, locale: AgentLocale): string {
	if (locale === 'zh') {
		return `${systemMd}

${languageDirective(locale)}

你一次只执行一个分析步骤，最多调用一个工具。
若本步证据已足够，设 done=true 且 tool=none。
写 SQL 时使用 tool=query，且只能是单条只读语句。
参数键名使用 camelCase：connectionId、schema、table、sql、limit、offset、kind、name。`;
	}
	return `${systemMd}

${languageDirective(locale)}

You execute ONE analysis step using at most one tool call.
If you already have enough evidence for this step, set done=true and tool=none.
For SQL use tool=query with a single read-only statement.
Args must use camelCase keys: connectionId, schema, table, sql, limit, offset, kind, name.`;
}

export function actUserPrompt(input: {
	query: string;
	schemaDigest: string;
	step: AnalystPlanStep;
	stepIndex: number;
	total: number;
	observations: AnalystObservation[];
	locale: AgentLocale;
}): string {
	const prior = input.observations
		.slice(-6)
		.map(
			(item) =>
				`- step ${item.stepIndex + 1} ${item.tool} ok=${item.ok}: ${item.summary.slice(0, 500)}`
		)
		.join('\n');
	if (input.locale === 'zh') {
		return `用户问题：
${input.query}

Schema 摘要：
${input.schemaDigest}

当前步骤 ${input.stepIndex + 1}/${input.total}：
title: ${input.step.title}
goal: ${input.step.goal}
connectionHint: ${input.step.connectionHint ?? ''}
tablesHint: ${(input.step.tablesHint ?? []).join(', ')}
toolHint: ${input.step.toolHint ?? ''}
sqlIntent: ${input.step.sqlIntent ?? ''}

近期观察：
${prior || '（无）'}

为本步决定下一次工具调用，或标记 done=true。`;
	}
	return `User question:
${input.query}

Schema digest:
${input.schemaDigest}

Current step ${input.stepIndex + 1}/${input.total}:
title: ${input.step.title}
goal: ${input.step.goal}
connectionHint: ${input.step.connectionHint ?? ''}
tablesHint: ${(input.step.tablesHint ?? []).join(', ')}
toolHint: ${input.step.toolHint ?? ''}
sqlIntent: ${input.step.sqlIntent ?? ''}

Recent observations:
${prior || '(none)'}

Decide the next tool call for this step, or mark done=true.`;
}

export function synthesizeSystemPrompt(systemMd: string, locale: AgentLocale): string {
	if (locale === 'zh') {
		return `${systemMd}

${languageDirective(locale)}

用 Markdown 给用户写最终回答。
包含：简短总结、关键发现、证据（表 / SQL）、注意事项。
不要编造未观察到的数字。`;
	}
	return `${systemMd}

${languageDirective(locale)}

Write the final answer in Markdown for the user.
Include: short summary, key findings, evidence (tables / SQL), and caveats.
Do not invent numbers that were not observed.`;
}

export function synthesizeUserPrompt(input: {
	query: string;
	plan: AnalystPlanStep[];
	observations: AnalystObservation[];
	locale: AgentLocale;
}): string {
	const planText = input.plan
		.map((step, index) => `${index + 1}. ${step.title} — ${step.goal}`)
		.join('\n');
	const obs = input.observations
		.map(
			(item) =>
				`### Step ${item.stepIndex + 1} · ${item.tool} · ${item.ok ? 'ok' : 'failed'}\nArgs: \`${JSON.stringify(item.args)}\`\n\n\`\`\`\n${item.summary.slice(0, 3000)}\n\`\`\``
		)
		.join('\n\n');
	if (input.locale === 'zh') {
		return `用户问题：
${input.query}

计划：
${planText}

观察：
${obs || '（无）'}

返回带 markdown 字段的 JSON。`;
	}
	return `User question:
${input.query}

Plan:
${planText}

Observations:
${obs || '(none)'}

Return JSON with markdown.`;
}

export function statusMessage(
	locale: AgentLocale,
	key: 'planning' | 'writing' | 'gathering'
): string {
	if (locale === 'zh') {
		switch (key) {
			case 'gathering':
				return '正在收集连接与 schema…';
			case 'planning':
				return '正在制定分析计划…';
			case 'writing':
				return '正在撰写分析结论…';
		}
	}
	switch (key) {
		case 'gathering':
			return 'Gathering connections and schema…';
		case 'planning':
			return 'Designing analysis plan…';
		case 'writing':
			return 'Writing analysis…';
	}
}

export function fallbackStepNote(locale: AgentLocale, done: boolean): string {
	if (locale === 'zh') {
		return done ? '本步完成' : '继续';
	}
	return done ? 'Step complete' : 'Continuing';
}
