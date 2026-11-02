import { z } from 'zod';
import type { AnalystObservation, AnalystPlanStep } from './events.js';

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

export function planSystemPrompt(systemMd: string): string {
	return `${systemMd}

You design a short multi-step analysis plan. Each step should be concrete and tool-backed.
Prefer discovering schema before writing SQL. Cap at 8 steps.`;
}

export function planUserPrompt(query: string, schemaDigest: string): string {
	return `User question:
${query}

Schema digest (connections / databases / sample tables):
${schemaDigest}

Return JSON steps only.`;
}

export function actSystemPrompt(systemMd: string): string {
	return `${systemMd}

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
}): string {
	const prior = input.observations
		.slice(-6)
		.map(
			(item) =>
				`- step ${item.stepIndex + 1} ${item.tool} ok=${item.ok}: ${item.summary.slice(0, 500)}`
		)
		.join('\n');
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

export function synthesizeSystemPrompt(systemMd: string): string {
	return `${systemMd}

Write the final answer in Markdown for the user.
Include: short summary, key findings, evidence (tables / SQL), and caveats.
Do not invent numbers that were not observed.`;
}

export function synthesizeUserPrompt(input: {
	query: string;
	plan: AnalystPlanStep[];
	observations: AnalystObservation[];
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
	return `User question:
${input.query}

Plan:
${planText}

Observations:
${obs || '(none)'}

Return JSON with markdown.`;
}
