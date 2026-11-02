import { api } from '$lib/api/tauri';
import type { AgentToolResult } from '$lib/api/types';

export const SKILL_TOOLS = [
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
	'query'
] as const;

export type SkillToolName = (typeof SKILL_TOOLS)[number];

export async function callSkillTool(
	name: SkillToolName | string,
	args: Record<string, unknown> = {}
): Promise<AgentToolResult> {
	return api.agentToolCall(name, args);
}

export function summarizeToolResult(name: string, result: AgentToolResult): string {
	if (!result.ok) return result.error?.trim() || 'tool failed';
	const value = result.result;
	try {
		const text = JSON.stringify(value);
		if (text.length <= 2400) return text;
		return `${text.slice(0, 2400)}…`;
	} catch {
		return `${name} ok`;
	}
}
