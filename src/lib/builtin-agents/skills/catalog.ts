/** User-facing slash skills that map onto HiDataLinker tool allowlists. */

export type AgentSkillDef = {
	id: string;
	/** Token after `/` in the composer (e.g. `schema`). */
	slash: string;
	/** Tool names from the Rust agent catalog. */
	tools: readonly string[];
};

export const AGENT_SKILLS: readonly AgentSkillDef[] = [
	{
		id: 'connections',
		slash: 'connections',
		tools: ['list_connections', 'connect']
	},
	{
		id: 'schema',
		slash: 'schema',
		tools: [
			'list_databases',
			'list_tables',
			'list_views',
			'list_indexes',
			'list_triggers',
			'list_routines',
			'get_columns',
			'get_ddl'
		]
	},
	{
		id: 'preview',
		slash: 'preview',
		tools: ['preview_table']
	},
	{
		id: 'sql',
		slash: 'sql',
		tools: ['query']
	},
	{
		id: 'script',
		slash: 'script',
		tools: ['write_script', 'run_script', 'list_scripts']
	}
] as const;

export type AgentSkillId = (typeof AGENT_SKILLS)[number]['id'];

export function findSkill(idOrSlash: string): AgentSkillDef | undefined {
	const key = idOrSlash.trim().toLowerCase();
	if (!key) return undefined;
	return AGENT_SKILLS.find((item) => item.id === key || item.slash === key);
}

export function resolveSkillTools(skillIds: string[]): string[] {
	const tools = new Set<string>();
	for (const raw of skillIds) {
		const skill = findSkill(raw);
		if (!skill) continue;
		for (const tool of skill.tools) tools.add(tool);
	}
	return [...tools];
}
