<script lang="ts">
	import {
		Database,
		History,
		Lightbulb,
		LoaderCircle,
		MessageSquare,
		PanelLeftClose,
		PanelLeftOpen
	} from '@lucide/svelte';
	import Switch from '$lib/components/layout/Switch.svelte';
	import { DEMO_FALLBACK_SOURCES } from '$lib/agent/demo';
	import { engineLabel } from '$lib/engine';
	import { t, type MessageKey } from '$lib/i18n/i18n.svelte';
	import { appMode, type AgentNav } from '$lib/stores/app-mode.svelte';
	import { insights } from '$lib/stores/insights.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';

	const navItems: { id: AgentNav; icon: typeof MessageSquare; labelKey: MessageKey }[] = [
		{ id: 'chat', icon: MessageSquare, labelKey: 'agentMode.nav.chat' },
		{ id: 'sources', icon: Database, labelKey: 'agentMode.nav.sources' },
		{ id: 'insights', icon: Lightbulb, labelKey: 'agentMode.nav.insights' },
		{ id: 'history', icon: History, labelKey: 'agentMode.nav.history' }
	];

	const expanded = $derived(appMode.prefs.sidebarExpanded);

	const sources = $derived.by(() => {
		const real = workspace.connections.map((c) => ({
			id: c.id,
			engine: c.engine,
			name: c.name,
			connected: c.connected
		}));
		if (real.length > 0) return real;
		return DEMO_FALLBACK_SOURCES.map((s) => ({
			id: s.id,
			engine: s.engine,
			name: s.name,
			connected: s.connected
		}));
	});

	function selectNav(id: AgentNav) {
		appMode.setAgentNav(id);
		if (id === 'chat') {
			appMode.sessionId = 'home';
		}
	}

	function toggle() {
		appMode.patchPrefs({ sidebarExpanded: !appMode.prefs.sidebarExpanded });
	}
</script>

<aside class="agent-sidebar" class:collapsed={!expanded}>
	<div class="agent-sidebar-section">
		<div class="agent-sidebar-label">
			{#if expanded}
				<span class="agent-sidebar-label-text">{t('agentMode.workspace')}</span>
			{/if}
			<button
				type="button"
				class="agent-sidebar-toggle"
				title={expanded ? t('agentMode.sidebar.collapse') : t('agentMode.sidebar.expand')}
				aria-label={expanded ? t('agentMode.sidebar.collapse') : t('agentMode.sidebar.expand')}
				aria-expanded={expanded}
				onclick={toggle}
			>
				{#if expanded}
					<PanelLeftClose size={15} />
				{:else}
					<PanelLeftOpen size={15} />
				{/if}
			</button>
		</div>
		<nav class="agent-nav">
			{#each navItems as item (item.id)}
				{@const Icon = item.icon}
				<button
					type="button"
					class="agent-nav-item"
					class:active={appMode.agentNav === item.id}
					title={expanded ? undefined : t(item.labelKey)}
					aria-label={t(item.labelKey)}
					onclick={() => selectNav(item.id)}
				>
					<Icon size={15} />
					{#if expanded}
						<span>{t(item.labelKey)}</span>
					{/if}
					{#if item.id === 'insights' && insights.running}
						<LoaderCircle size={12} class="agent-spin agent-nav-busy" />
					{/if}
				</button>
			{/each}
		</nav>
	</div>

	<div class="agent-sidebar-section agent-sidebar-sources">
		<div class="agent-sidebar-label">
			{#if expanded}
				{t('agentMode.sources')}
			{/if}
			<span class="agent-sidebar-count">{sources.length}</span>
		</div>
		<ul class="agent-source-list">
			{#each sources as src (src.id)}
				<li
					class="agent-source-row"
					title={expanded ? undefined : `${engineLabel(src.engine)} · ${src.name}`}
				>
					<span class="agent-source-dot" class:on={src.connected}></span>
					{#if expanded}
						<div class="agent-source-meta">
							<span class="agent-source-engine">{engineLabel(src.engine)}</span>
							<span class="agent-source-name truncate">{src.name}</span>
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	</div>

	{#if expanded}
		<div class="agent-sidebar-footer">
			<div class="agent-auto-card">
				<div class="agent-auto-head">
					<span>{t('agentMode.autoConnect.title')}</span>
					<Switch
						name={t('agentMode.autoConnect.title')}
						checked={appMode.prefs.autoConnect}
						onText={t('settings.mcp.on')}
						offText={t('settings.mcp.off')}
						onToggle={(enabled) => appMode.patchPrefs({ autoConnect: enabled })}
					/>
				</div>
				<p>{t('agentMode.autoConnect.hint')}</p>
			</div>
		</div>
	{/if}
</aside>
