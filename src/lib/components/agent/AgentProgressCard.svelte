<script lang="ts">
	import { LoaderCircle } from '@lucide/svelte';
	import type { ChatMessage } from '$lib/stores/agent-chat.svelte';
	import { t } from '$lib/i18n/i18n.svelte';

	let { message, live = false }: { message: ChatMessage; live?: boolean } = $props();

	const tool = $derived(typeof message.meta?.tool === 'string' ? message.meta.tool : null);
	const ok = $derived(message.meta?.ok === true);
</script>

{#if message.kind === 'tool'}
	<article class="agent-progress-card tool" class:fail={message.meta?.ok === false}>
		<header>
			<strong>{tool ?? 'tool'}</strong>
			<span>
				{#if ok}{t('agentMode.live.toolOk')}{:else}{t('agentMode.live.toolFail')}{/if}
			</span>
		</header>
		{#if message.content}
			<pre>{message.content}</pre>
		{/if}
		{#if typeof message.meta?.summary === 'string' && message.meta.summary !== message.content}
			<details>
				<summary>{t('agentMode.live.toolResult')}</summary>
				<pre>{message.meta.summary}</pre>
			</details>
		{/if}
	</article>
{:else}
	<article class="agent-progress-card">
		<header>
			{#if live}
				<span class="agent-spin" aria-hidden="true"><LoaderCircle size={14} /></span>
			{/if}
			<span>{message.content}</span>
		</header>
	</article>
{/if}
