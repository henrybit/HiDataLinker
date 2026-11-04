<script lang="ts">
	import { ChevronRight, LoaderCircle } from '@lucide/svelte';
	import type { ChatMessage } from '$lib/stores/agent-chat.svelte';
	import { t } from '$lib/i18n/i18n.svelte';
	import AgentProgressCard from './AgentProgressCard.svelte';

	let {
		messages,
		live = false
	}: {
		messages: ChatMessage[];
		live?: boolean;
	} = $props();

	let open = $state(live);

	$effect(() => {
		open = live;
	});

	const toolCount = $derived(messages.filter((item) => item.kind === 'tool').length);
	const stepCount = $derived(messages.length - toolCount);
</script>

<details class="agent-process-group" class:live bind:open>
	<summary class="agent-process-summary">
		<span class="agent-process-chevron" aria-hidden="true"><ChevronRight size={14} /></span>
		{#if live}
			<span class="agent-spin" aria-hidden="true"><LoaderCircle size={14} /></span>
			<span>{t('agentMode.live.processRunning')}</span>
		{:else}
			<span>{t('agentMode.live.processCollapsed', { count: messages.length })}</span>
			{#if toolCount > 0 || stepCount > 0}
				<span class="agent-process-meta">
					{#if stepCount > 0}{t('agentMode.live.processSteps', { count: stepCount })}{/if}
					{#if toolCount > 0 && stepCount > 0}
						·
					{/if}
					{#if toolCount > 0}{t('agentMode.live.processTools', { count: toolCount })}{/if}
				</span>
			{/if}
		{/if}
	</summary>
	<div class="agent-process-body">
		{#each messages as message (message.id)}
			<AgentProgressCard {message} live={live && message.meta?.phase === 'status'} />
		{/each}
	</div>
</details>
