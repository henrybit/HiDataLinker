<script lang="ts">
	import {
		ChevronLeft,
		ChevronRight,
		MessageSquare,
		PanelLeftClose,
		PanelLeftOpen,
		Plus,
		LoaderCircle
	} from '@lucide/svelte';
	import { getLocale, t } from '$lib/i18n/i18n.svelte';
	import { agentChat } from '$lib/stores/agent-chat.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import type { AgentSessionMeta } from '$lib/api/types';

	const expanded = $derived(appMode.prefs.sessionRailExpanded);

	const sessions = $derived.by(() => {
		const list = [...agentChat.history];
		const current = agentChat.session;
		if (current && !list.some((item) => item.id === current.id)) {
			list.unshift(current);
		}
		return list;
	});

	function toggle() {
		appMode.patchPrefs({ sessionRailExpanded: !appMode.prefs.sessionRailExpanded });
	}

	function formatWhen(iso: string): string {
		const date = new Date(iso);
		if (Number.isNaN(date.getTime())) return '';
		const locale = getLocale() === 'zh' ? 'zh-CN' : 'en-US';
		const now = Date.now();
		const diff = now - date.getTime();
		if (diff < 60_000) return t('agentMode.sessions.justNow');
		if (diff < 3_600_000) {
			return t('agentMode.sessions.minutesAgo', { count: Math.max(1, Math.floor(diff / 60_000)) });
		}
		if (diff < 86_400_000) {
			return t('agentMode.sessions.hoursAgo', { count: Math.max(1, Math.floor(diff / 3_600_000)) });
		}
		return new Intl.DateTimeFormat(locale, {
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit',
			hour12: false
		}).format(date);
	}

	function initial(title: string): string {
		const text = title.trim();
		return text ? text.slice(0, 1).toUpperCase() : '#';
	}

	function isActive(item: AgentSessionMeta): boolean {
		return agentChat.session?.id === item.id;
	}

	async function select(item: AgentSessionMeta) {
		appMode.sessionId = 'home';
		await agentChat.switchSession(item.id);
	}

	async function create() {
		appMode.sessionId = 'home';
		await agentChat.newChat();
	}
</script>

<aside
	class="agent-session-rail"
	class:expanded
	class:collapsed={!expanded}
	aria-label={t('agentMode.sessions.title')}
>
	<div class="agent-session-rail-head">
		{#if expanded}
			<strong>{t('agentMode.sessions.title')}</strong>
		{/if}
		<button
			type="button"
			class="agent-session-rail-icon-btn"
			title={expanded ? t('agentMode.sessions.collapse') : t('agentMode.sessions.expand')}
			aria-label={expanded ? t('agentMode.sessions.collapse') : t('agentMode.sessions.expand')}
			onclick={toggle}
		>
			{#if expanded}
				<PanelLeftClose size={16} />
			{:else}
				<PanelLeftOpen size={16} />
			{/if}
		</button>
	</div>

	<button
		type="button"
		class="agent-session-new"
		class:collapsed={!expanded}
		title={t('agentMode.history.newChat')}
		onclick={() => void create()}
	>
		<Plus size={16} />
		{#if expanded}
			<span>{t('agentMode.history.newChat')}</span>
		{/if}
	</button>

	<div class="agent-session-rail-list" role="list">
		{#if sessions.length === 0}
			{#if expanded}
				<p class="agent-session-rail-empty">{t('agentMode.sessions.empty')}</p>
			{/if}
		{:else}
			{#each sessions as item (item.id)}
				<button
					type="button"
					class="agent-session-rail-item"
					class:active={isActive(item)}
					class:collapsed={!expanded}
					role="listitem"
					title={item.title}
					onclick={() => void select(item)}
				>
					<span class="agent-session-rail-avatar" aria-hidden="true">
						{#if agentChat.isSessionRunning(item.id)}
							<span class="agent-spin"><LoaderCircle size={14} /></span>
						{:else}
							{initial(item.title)}
						{/if}
					</span>
					{#if expanded}
						<span class="agent-session-rail-meta">
							<span class="agent-session-rail-title">{item.title}</span>
							<span class="agent-session-rail-sub">
								{formatWhen(item.updatedAt)}
								{#if agentChat.isSessionRunning(item.id)}
									· {t('agentMode.sessions.running')}
								{/if}
							</span>
						</span>
						{#if isActive(item)}
							<span class="agent-session-rail-chevron" aria-hidden="true">
								<ChevronRight size={14} />
							</span>
						{/if}
					{/if}
				</button>
			{/each}
		{/if}
	</div>

	{#if !expanded}
		<button
			type="button"
			class="agent-session-rail-icon-btn bottom"
			title={t('agentMode.sessions.expand')}
			aria-label={t('agentMode.sessions.expand')}
			onclick={toggle}
		>
			<ChevronLeft size={16} />
		</button>
	{:else}
		<p class="agent-session-rail-hint">
			<MessageSquare size={12} />
			{t('agentMode.sessions.hint')}
		</p>
	{/if}
</aside>
