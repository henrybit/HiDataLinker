<script lang="ts">
	import { onMount } from 'svelte';
	import { MessageSquare, PenLine } from '@lucide/svelte';
	import { t } from '$lib/i18n/i18n.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import { agentChat } from '$lib/stores/agent-chat.svelte';

	onMount(() => {
		void agentChat.refreshHistory();
	});

	async function open(id: string) {
		appMode.setAgentNav('chat');
		appMode.sessionId = 'home';
		await agentChat.switchSession(id);
	}
</script>

<div class="agent-history-page">
	<header class="agent-page-head">
		<h2>{t('agentMode.history.title')}</h2>
		<p>{t('agentMode.history.desc')}</p>
	</header>
	{#if agentChat.history.length === 0}
		<p class="agent-hero-desc">{t('agentMode.live.noHistory')}</p>
	{:else}
		<ul class="agent-history-list">
			{#each agentChat.history as item (item.id)}
				<li>
					<button type="button" class="agent-history-item" onclick={() => open(item.id)}>
						<span class="agent-history-icon">
							{#if item.status === 'error'}
								<PenLine size={16} />
							{:else}
								<MessageSquare size={16} />
							{/if}
						</span>
						<div>
							<strong>{item.title}</strong>
							<span>{item.updatedAt} · {item.status}</span>
						</div>
					</button>
				</li>
			{/each}
		</ul>
	{/if}
	<button
		type="button"
		class="agent-link-btn"
		onclick={() => {
			appMode.setAgentNav('chat');
			appMode.openHome();
			void agentChat.newChat();
		}}
	>
		{t('agentMode.history.newChat')}
	</button>
</div>
