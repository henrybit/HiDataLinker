<script lang="ts">
	import { Database, GitBranch, ShieldCheck, Sparkles } from '@lucide/svelte';
	import { DEMO_SUGGESTIONS } from '$lib/agent/demo';
	import { t } from '$lib/i18n/i18n.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';
	import AgentComposer from './AgentComposer.svelte';

	const sourceCount = $derived(
		workspace.connections.length > 0 ? workspace.connections.length : 8
	);
</script>

<div class="agent-chat-home">
	<div class="agent-chat-hero">
		<div class="agent-hero-icon" aria-hidden="true">
			<Sparkles size={28} />
		</div>
		<h1>{t('agentMode.home.title')}</h1>
		<p class="agent-hero-desc">
			{t('agentMode.home.desc', { count: sourceCount })}
		</p>
		<div class="agent-hero-badges">
			<span><Database size={13} /> {t('agentMode.home.badgeSources', { count: sourceCount })}</span>
			<span><GitBranch size={13} /> {t('agentMode.home.badgeJoins')}</span>
			<span><ShieldCheck size={13} /> {t('agentMode.home.badgeWrite')}</span>
		</div>
		<div class="agent-suggest-row">
			{#each DEMO_SUGGESTIONS as item (item.id)}
				<button
					type="button"
					class="agent-suggest-chip"
					onclick={() => appMode.applySuggestion(item.session, t(item.labelKey))}
				>
					{t(item.labelKey)}
				</button>
			{/each}
		</div>
	</div>
	<AgentComposer
		enableSourceMention={false}
		enableSkillSlash={false}
		enableContextMeter={false}
		onSubmit={(text) => appMode.submitComposer(text)}
	/>
</div>
