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
			'list_indexes',
			'list_triggers',
			'list_routines',
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

function scopeDirective(locale: AgentLocale, connectionIds?: string[]): string {
	const ids = (connectionIds ?? []).map((id) => id.trim()).filter(Boolean);
	if (ids.length === 0) return '';
	if (locale === 'zh') {
		return `用户已用 @ 指定数据源，只能使用这些 connectionId：${ids.join(', ')}。不要查询其它连接。`;
	}
	return `The user @-mentioned these data sources only. Use solely these connectionId values: ${ids.join(', ')}. Do not query other connections.`;
}

function skillDirective(
	locale: AgentLocale,
	skillLabels: string[] | undefined,
	allowedTools: string[] | undefined
): string {
	const labels = (skillLabels ?? []).filter(Boolean);
	const tools = (allowedTools ?? []).filter(Boolean);
	if (labels.length === 0 || tools.length === 0) return '';
	if (locale === 'zh') {
		return `用户已用 / 指定技能：${labels.join(' ')}。本轮只能使用这些工具：${tools.join(', ')}（以及 none）。不要调用其它工具。`;
	}
	return `The user selected slash skills: ${labels.join(' ')}. For this turn only use these tools: ${tools.join(', ')} (plus none). Do not call other tools.`;
}

function directiveBlock(
	locale: AgentLocale,
	connectionIds?: string[],
	skillLabels?: string[],
	allowedTools?: string[]
): string {
	const parts = [
		scopeDirective(locale, connectionIds),
		skillDirective(locale, skillLabels, allowedTools)
	].filter(Boolean);
	if (parts.length === 0) return '';
	return `\n${parts.join('\n')}\n`;
}

function conversationBlock(locale: AgentLocale, conversationContext?: string): string {
	const text = conversationContext?.trim();
	if (!text) return '';
	if (locale === 'zh') {
		return `\n此前对话上下文（可能含压缩摘要）：\n${text}\n`;
	}
	return `\nPrior conversation context (may include a compressed summary):\n${text}\n`;
}

export function planUserPrompt(
	query: string,
	schemaDigest: string,
	locale: AgentLocale,
	connectionIds?: string[],
	conversationContext?: string,
	skillLabels?: string[],
	allowedTools?: string[]
): string {
	const directives = directiveBlock(locale, connectionIds, skillLabels, allowedTools);
	const prior = conversationBlock(locale, conversationContext);
	if (locale === 'zh') {
		return `用户问题：
${query}
${prior}${directives}
Schema 摘要（连接 / 库 / 样例表）：
${schemaDigest}

只返回 JSON steps。`;
	}
	return `User question:
${query}
${prior}${directives}
Schema digest (connections / databases / sample tables):
${schemaDigest}

Return JSON steps only.`;
}

export function actSystemPrompt(systemMd: string, locale: AgentLocale): string {
	if (locale === 'zh') {
		return `${systemMd}

${languageDirective(locale)}

你一次只决定「本步」的下一次动作，最多调用一个工具。
- 若本步目标尚未完成：设 done=false，并指定真实 tool（不可为 none）；运行时会带着新观察再次进入本步。
- 若本步证据已足够：设 done=true 且 tool=none。
- 禁止仅用 note 写「下一步：…」却设 tool=none；要把下一步变成实际 tool 调用。
写 SQL 时使用 tool=query，且只能是单条只读语句。
参数键名使用 camelCase：connectionId、schema、table、sql、limit、offset、kind、name。
必须通过 emit_analysis 工具或纯 JSON 返回（字段：done/note/tool/args），禁止输出 Markdown 分析正文。`;
	}
	return `${systemMd}

${languageDirective(locale)}

Decide the NEXT action for the CURRENT plan step; at most one tool call.
- If this step's goal is not yet met: set done=false and a real tool (not none); the runtime will re-enter this step with new observations.
- If you already have enough evidence for this step: set done=true and tool=none.
- Do not only write a "next:" note with tool=none — turn that into an actual tool call.
For SQL use tool=query with a single read-only statement.
Args must use camelCase keys: connectionId, schema, table, sql, limit, offset, kind, name.
You MUST reply via the emit_analysis tool or raw JSON only (fields: done/note/tool/args). Do not write Markdown analysis prose.`;
}

export function actUserPrompt(input: {
	query: string;
	schemaDigest: string;
	step: AnalystPlanStep;
	stepIndex: number;
	total: number;
	observations: AnalystObservation[];
	locale: AgentLocale;
	actIter?: number;
	maxActIters?: number;
	connectionIds?: string[];
	conversationContext?: string;
	skillLabels?: string[];
	allowedTools?: string[];
}): string {
	const prior = input.observations
		.slice(-8)
		.map(
			(item) =>
				`- step ${item.stepIndex + 1} ${item.tool} ok=${item.ok}: ${item.summary.slice(0, 500)}`
		)
		.join('\n');
	const iter =
		typeof input.actIter === 'number' && typeof input.maxActIters === 'number'
			? input.locale === 'zh'
				? `本步内第 ${input.actIter + 1}/${input.maxActIters} 次动作。`
				: `Action ${input.actIter + 1}/${input.maxActIters} within this step.`
			: '';
	const directives = directiveBlock(
		input.locale,
		input.connectionIds,
		input.skillLabels,
		input.allowedTools
	);
	const history = conversationBlock(input.locale, input.conversationContext);
	if (input.locale === 'zh') {
		return `用户问题：
${input.query}
${history}${directives}
Schema 摘要：
${input.schemaDigest}

当前步骤 ${input.stepIndex + 1}/${input.total}：
title: ${input.step.title}
goal: ${input.step.goal}
connectionHint: ${input.step.connectionHint ?? ''}
tablesHint: ${(input.step.tablesHint ?? []).join(', ')}
toolHint: ${input.step.toolHint ?? ''}
sqlIntent: ${input.step.sqlIntent ?? ''}
${iter}

近期观察：
${prior || '（无）'}

为本步决定下一次工具调用（done=false），或标记本步完成（done=true, tool=none）。
只返回 JSON / 工具调用，不要写「本步目标」「判断」等 Markdown。`;
	}
	return `User question:
${input.query}
${history}${directives}
Schema digest:
${input.schemaDigest}

Current step ${input.stepIndex + 1}/${input.total}:
title: ${input.step.title}
goal: ${input.step.goal}
connectionHint: ${input.step.connectionHint ?? ''}
tablesHint: ${(input.step.tablesHint ?? []).join(', ')}
toolHint: ${input.step.toolHint ?? ''}
sqlIntent: ${input.step.sqlIntent ?? ''}
${iter}

Recent observations:
${prior || '(none)'}

Decide the next tool call for this step (done=false), or mark the step complete (done=true, tool=none).
Return JSON / tool call only — no Markdown analysis.`;
}

export function synthesizeSystemPrompt(systemMd: string, locale: AgentLocale): string {
	if (locale === 'zh') {
		return `${systemMd}

${languageDirective(locale)}

通过 emit_analysis 返回 JSON：{"markdown":"..."}。markdown 字段里写给用户的最终回答（标题、列表、表格、链接、代码块；需要时可内嵌 HTML，或使用 \`\`\`html\`\`\` 代码块）。
包含：简短总结、关键发现、证据（表 / SQL）、注意事项。
不要编造未观察到的数字。
若观察结果适合可视化（对比、趋势、占比），在 markdown 中额外附加一个或多个 \`\`\`chart\`\`\` JSON 代码块，格式如下之一：
{"type":"bar"|"line"|"area","title":"...","categories":["A","B"],"series":[{"name":"指标","data":[1,2]}]}
{"type":"pie","title":"...","data":[{"name":"A","value":1}]}
图表数据必须来自观察，禁止臆造。
不要直接输出裸 Markdown；必须包在 JSON 的 markdown 字段中。`;
	}
	return `${systemMd}

${languageDirective(locale)}

Return JSON via emit_analysis: {"markdown":"..."}. Put the user-facing final answer inside the markdown field (headings, lists, tables, links, code; HTML inline or in \`\`\`html\`\`\` fences when useful).
Include: short summary, key findings, evidence (tables / SQL), and caveats.
Do not invent numbers that were not observed.
When observations are chartable (comparison, trend, share), also append one or more \`\`\`chart\`\`\` JSON fences inside markdown:
{"type":"bar"|"line"|"area","title":"...","categories":["A","B"],"series":[{"name":"metric","data":[1,2]}]}
{"type":"pie","title":"...","data":[{"name":"A","value":1}]}
Chart values must come from observations only.
Do not emit bare Markdown outside the JSON markdown field.`;
}

export function synthesizeUserPrompt(input: {
	query: string;
	plan: AnalystPlanStep[];
	observations: AnalystObservation[];
	locale: AgentLocale;
	conversationContext?: string;
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
	const history = conversationBlock(input.locale, input.conversationContext);
	if (input.locale === 'zh') {
		return `用户问题：
${input.query}
${history}
计划：
${planText}

观察：
${obs || '（无）'}

返回带 markdown 字段的 JSON。`;
	}
	return `User question:
${input.query}
${history}
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
