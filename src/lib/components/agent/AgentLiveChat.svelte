<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { Sparkles, Square } from '@lucide/svelte';
	import { DEMO_SUGGESTIONS } from '$lib/agent/demo';
	import { t } from '$lib/i18n/i18n.svelte';
	import { agentChat } from '$lib/stores/agent-chat.svelte';
	import AgentComposer from './AgentComposer.svelte';
	import AgentMarkdown from './AgentMarkdown.svelte';
	import AgentProgressCard from './AgentProgressCard.svelte';

	let scroller = $state<HTMLDivElement | null>(null);

	async function scrollBottom() {
		await tick();
		if (scroller) scroller.scrollTop = scroller.scrollHeight;
	}

	$effect(() => {
		agentChat.messages.length;
		agentChat.running;
		void scrollBottom();
	});

	onMount(() => {
		void agentChat.ensureSubscribed();
		void agentChat.refreshHistory();
	});
</script>

<div class="agent-live-chat">
	{#if agentChat.messages.length === 0}
		<div class="agent-chat-hero">
			<div class="agent-hero-icon" aria-hidden="true">
				<Sparkles size={28} />
			</div>
			<h1>{t('agentMode.home.title')}</h1>
			<p class="agent-hero-desc">{t('agentMode.live.homeDesc')}</p>
			<div class="agent-suggest-row">
				{#each DEMO_SUGGESTIONS as item (item.id)}
					<button
						type="button"
						class="agent-suggest-chip"
						disabled={agentChat.running}
						onclick={() => {
							agentChat.draft = t(item.labelKey);
							void agentChat.submit(t(item.labelKey));
						}}
					>
						{t(item.labelKey)}
					</button>
				{/each}
			</div>
		</div>
	{:else}
		<header class="agent-session-head">
			<h2>{agentChat.session?.title ?? t('agentMode.nav.chat')}</h2>
			{#if agentChat.running}
				<button type="button" class="agent-btn ghost" onclick={() => agentChat.cancel()}>
					<Square size={12} />
					{t('agentMode.live.cancel')}
				</button>
			{:else}
				<button type="button" class="agent-btn ghost" onclick={() => agentChat.newChat()}>
					{t('agentMode.history.newChat')}
				</button>
			{/if}
		</header>
		<div class="agent-session-scroll" bind:this={scroller}>
			{#each agentChat.messages as message (message.id)}
				{#if message.role === 'user'}
					<div class="agent-bubble user">{message.content}</div>
				{:else if message.kind === 'markdown'}
					<div class="agent-agent-block">
						<div class="agent-agent-label">
							<span class="agent-agent-avatar"><Sparkles size={14} /></span>
							HiDataLinker Agent
						</div>
						<AgentMarkdown content={message.content} />
					</div>
				{:else if message.kind === 'error'}
					<div class="agent-error-banner">{message.content}</div>
				{:else}
					<AgentProgressCard
						{message}
						live={agentChat.running && message.meta?.phase === 'status'}
					/>
				{/if}
			{/each}
		</div>
	{/if}

	{#if agentChat.error && agentChat.messages.length === 0}
		<p class="agent-error-banner inline">{agentChat.error}</p>
	{/if}

	<AgentComposer
		bind:value={agentChat.draft}
		disabled={agentChat.running}
		placeholder={t('agentMode.composer.placeholder')}
		hint={agentChat.running ? t('agentMode.live.runningHint') : t('agentMode.composer.hint')}
		onSubmit={() => agentChat.submit()}
	/>
</div>
