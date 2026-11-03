<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { Minimize2, Sparkles, Square } from '@lucide/svelte';
	import { DEMO_SUGGESTIONS } from '$lib/agent/demo';
	import { messageCopyText, messageDurationMs } from '$lib/agent/message-meta';
	import { findSkill } from '$lib/builtin-agents/skills/catalog';
	import { t } from '$lib/i18n/i18n.svelte';
	import { agentChat, type ChatMessage } from '$lib/stores/agent-chat.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';
	import AgentComposer from './AgentComposer.svelte';
	import AgentMarkdown from './AgentMarkdown.svelte';
	import AgentMessageMeta from './AgentMessageMeta.svelte';
	import AgentProgressCard from './AgentProgressCard.svelte';
	import AgentSessionRail from './AgentSessionRail.svelte';

	let scroller = $state<HTMLDivElement | null>(null);

	async function scrollBottom() {
		await tick();
		if (scroller) scroller.scrollTop = scroller.scrollHeight;
	}

	$effect(() => {
		agentChat.messages.length;
		agentChat.running;
		agentChat.queueLength;
		void scrollBottom();
	});

	const composerHint = $derived(
		agentChat.queueLength > 0
			? t('agentMode.live.queueHint', { count: agentChat.queueLength })
			: agentChat.running
				? t('agentMode.live.runningHint')
				: t('agentMode.composer.hint')
	);

	const viewingRunning = $derived(
		agentChat.isSessionRunning(agentChat.session?.id) || agentChat.running
	);

	function showMeta(message: ChatMessage): boolean {
		return message.content.trim().length > 0 || message.kind === 'tool';
	}

	function sourceLabels(message: ChatMessage): string[] {
		const raw = message.meta?.connectionIds;
		if (!Array.isArray(raw)) return [];
		return raw
			.map((id) => {
				if (typeof id !== 'string' || !id.trim()) return '';
				const conn = workspace.connections.find((item) => item.id === id);
				return conn?.name ?? id;
			})
			.filter(Boolean);
	}

	function skillChips(message: ChatMessage): { slash: string; label: string }[] {
		const raw = message.meta?.skillIds;
		if (!Array.isArray(raw)) return [];
		return raw
			.map((id) => {
				if (typeof id !== 'string' || !id.trim()) return null;
				const skill = findSkill(id);
				if (!skill) return { slash: id, label: id };
				return {
					slash: skill.slash,
					label: t(`agentMode.skills.${skill.id}.label`)
				};
			})
			.filter((item): item is { slash: string; label: string } => item != null);
	}

	onMount(() => {
		void agentChat.ensureSubscribed();
		void agentChat.refreshHistory();
	});
</script>

<div
	class="agent-live-chat"
	class:rail-expanded={appMode.prefs.sessionRailExpanded}
	class:rail-collapsed={!appMode.prefs.sessionRailExpanded}
>
	<AgentSessionRail />

	<div class="agent-live-main">
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
							onclick={() => void agentChat.submit(t(item.labelKey))}
						>
							{t(item.labelKey)}
						</button>
					{/each}
				</div>
			</div>
		{:else}
			<header class="agent-session-head">
				<h2>{agentChat.session?.title ?? t('agentMode.nav.chat')}</h2>
				<div class="agent-session-actions">
					<label class="agent-chart-engine-inline">
						<span>{t('agentMode.chart.picker')}</span>
						<select
							aria-label={t('agentMode.chart.picker')}
							value={appMode.prefs.chartEngine}
							onchange={(event) => {
								const value = (event.currentTarget as HTMLSelectElement).value;
								if (value === 'echarts' || value === 'antd') {
									appMode.patchPrefs({ chartEngine: value });
								}
							}}
						>
							<option value="echarts">{t('agentMode.chart.engineEcharts')}</option>
							<option value="antd">{t('agentMode.chart.engineAntd')}</option>
						</select>
					</label>
					{#if viewingRunning}
						<button type="button" class="agent-btn ghost" onclick={() => agentChat.cancel()}>
							<Square size={12} />
							{t('agentMode.live.cancel')}
						</button>
					{:else}
						<button
							type="button"
							class="agent-btn ghost"
							disabled={agentChat.compressing || agentChat.messages.length < 4}
							title={t('agentMode.live.compressHint')}
							onclick={() => void agentChat.compressContext()}
						>
							<Minimize2 size={12} />
							{agentChat.compressing
								? t('agentMode.live.compressing')
								: t('agentMode.live.compress')}
						</button>
						<button type="button" class="agent-btn ghost" onclick={() => agentChat.newChat()}>
							{t('agentMode.history.newChat')}
						</button>
					{/if}
				</div>
			</header>
			{#if agentChat.contextNotice}
				<p class="agent-context-notice">{agentChat.contextNotice}</p>
			{/if}
			<div class="agent-session-scroll" bind:this={scroller}>
				{#each agentChat.messages as message (message.id)}
					{#if message.role === 'user'}
						<div class="agent-msg user">
							{#if skillChips(message).length > 0 || sourceLabels(message).length > 0}
								<div class="agent-msg-sources">
									{#each skillChips(message) as skill (skill.slash)}
										<span class="agent-msg-skill-chip" title={skill.label}
											>/{skill.slash}</span
										>
									{/each}
									{#each sourceLabels(message) as name (name)}
										<span class="agent-msg-source-chip">@{name}</span>
									{/each}
								</div>
							{/if}
							<div class="agent-bubble user" class:queued={message.meta?.queued === true}>
								<AgentMarkdown content={message.content} variant="user" />
								{#if message.meta?.queued === true}
									<span class="agent-queued-tag">{t('agentMode.live.queued')}</span>
								{/if}
							</div>
							{#if showMeta(message)}
								<AgentMessageMeta
									at={message.at}
									text={messageCopyText(message)}
									align="end"
								/>
							{/if}
						</div>
					{:else if message.kind === 'markdown'}
						<div class="agent-msg assistant">
							<div class="agent-agent-block">
								<div class="agent-agent-label">
									<span class="agent-agent-avatar"><Sparkles size={14} /></span>
									HiDataLinker Agent
								</div>
								<AgentMarkdown content={message.content} />
							</div>
							{#if showMeta(message)}
								<AgentMessageMeta
									at={message.at}
									durationMs={messageDurationMs(message)}
									text={messageCopyText(message)}
								/>
							{/if}
						</div>
					{:else if message.kind === 'error'}
						<div class="agent-msg assistant">
							<div class="agent-error-banner">{message.content}</div>
							{#if showMeta(message)}
								<AgentMessageMeta
									at={message.at}
									durationMs={messageDurationMs(message)}
									text={messageCopyText(message)}
								/>
							{/if}
						</div>
					{:else}
						<div class="agent-msg assistant">
							<AgentProgressCard
								{message}
								live={viewingRunning && message.meta?.phase === 'status'}
							/>
							{#if showMeta(message)}
								<AgentMessageMeta
									at={message.at}
									durationMs={messageDurationMs(message)}
									text={messageCopyText(message)}
								/>
							{/if}
						</div>
					{/if}
				{/each}
			</div>
		{/if}

		{#if agentChat.error && agentChat.messages.length === 0}
			<p class="agent-error-banner inline">{agentChat.error}</p>
		{/if}

		<AgentComposer
			placeholder={t('agentMode.composer.placeholder')}
			hint={composerHint}
			clearSignal={agentChat.composerClearSignal}
			onSubmit={(text, connectionIds, skillIds) =>
				void agentChat.submit(text, connectionIds, skillIds)}
		/>
	</div>
</div>
