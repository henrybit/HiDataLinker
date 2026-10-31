<script lang="ts">
	import { MessageSquare, PenLine } from '@lucide/svelte';
	import { DEMO_HISTORY } from '$lib/agent/demo';
	import { t } from '$lib/i18n/i18n.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';

	function open(id: string, kind: 'analysis' | 'write') {
		if (kind === 'write') appMode.openSession('write-orders');
		else appMode.openSession('analysis-revenue');
	}
</script>

<div class="agent-history-page">
	<header class="agent-page-head">
		<h2>{t('agentMode.history.title')}</h2>
		<p>{t('agentMode.history.desc')}</p>
	</header>
	<ul class="agent-history-list">
		{#each DEMO_HISTORY as item, i (i)}
			<li>
				<button type="button" class="agent-history-item" onclick={() => open(item.id, item.kind)}>
					<span class="agent-history-icon">
						{#if item.kind === 'write'}
							<PenLine size={16} />
						{:else}
							<MessageSquare size={16} />
						{/if}
					</span>
					<div>
						<strong>{t(item.titleKey)}</strong>
						<span>{t(item.whenKey)}</span>
					</div>
				</button>
			</li>
		{/each}
	</ul>
	<button type="button" class="agent-link-btn" onclick={() => appMode.openHome()}>
		{t('agentMode.history.newChat')}
	</button>
</div>
