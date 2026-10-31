<script lang="ts">
	import { onMount } from 'svelte';
	import {
		Cable,
		ChevronDown,
		Database,
		Play,
		Plus,
		RefreshCw,
		Settings,
		Sparkles,
		Square
	} from '@lucide/svelte';
	import { agentGateway, refreshAgentGateway } from '$lib/agent/gateway.svelte';
	import { analysisPanel } from '$lib/analysis/panel.svelte';
	import { llmSettingsDialog } from '$lib/llm/catalog.svelte';
	import { engineLabel } from '$lib/engine';
	import { getLocale, schemaNounLabel, setLocale, t, type Locale } from '$lib/i18n/i18n.svelte';
	import { migrationPanel } from '$lib/migration/panel.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';
	import { isTauriRuntime } from '$lib/api/tauri';

	const selected = $derived(workspace.selection);
	const active = $derived(
		selected ? workspace.connections.find((item) => item.id === selected.connectionId) : null
	);
	const locale = $derived(getLocale());
	const mcp = $derived(agentGateway.status);
	let menuOpen = $state(false);
	let menuRoot = $state<HTMLDivElement | null>(null);
	let migrationMenuOpen = $state(false);
	let migrationMenuRoot = $state<HTMLDivElement | null>(null);

	function switchLocale(next: Locale) {
		setLocale(next);
	}

	function openAnalysis() {
		menuOpen = false;
		analysisPanel.open = true;
	}

	function openMigration() {
		migrationMenuOpen = false;
		migrationPanel.open = true;
	}

	onMount(() => {
		if (isTauriRuntime()) void refreshAgentGateway();
		const onPointerDown = (event: PointerEvent) => {
			if (
				menuOpen &&
				menuRoot &&
				!(event.target instanceof Node && menuRoot.contains(event.target))
			) {
				menuOpen = false;
			}
			if (
				migrationMenuOpen &&
				migrationMenuRoot &&
				!(event.target instanceof Node && migrationMenuRoot.contains(event.target))
			) {
				migrationMenuOpen = false;
			}
		};
		window.addEventListener('pointerdown', onPointerDown);
		return () => window.removeEventListener('pointerdown', onPointerDown);
	});
</script>

<header class="toolbar" class:agent-mode={appMode.mode === 'agent'}>
	<div class="toolbar-brand">{t('app.title')}</div>

	<div class="mode-switch" role="group" aria-label={t('agentMode.modeSwitch')}>
		<button
			type="button"
			class="mode-switch-btn"
			class:active={appMode.mode === 'database'}
			onclick={() => appMode.setMode('database')}
		>
			{t('agentMode.mode.database')}
		</button>
		<button
			type="button"
			class="mode-switch-btn agent"
			class:active={appMode.mode === 'agent'}
			onclick={() => appMode.setMode('agent')}
		>
			<Sparkles size={13} />
			{t('agentMode.mode.agent')}
		</button>
	</div>

	{#if appMode.mode === 'database'}
		<div class="toolbar-sep"></div>
		<button class="toolbar-btn primary" onclick={() => workspace.openNewConnection()}>
			<Plus size={14} />
			{t('toolbar.newConnection')}
		</button>
		<button
			class="toolbar-btn"
			disabled={!active || active.connected || workspace.isPending(`connect:${active.id}`)}
			onclick={() => active && workspace.connect(active.id)}
		>
			<Cable size={14} />
			{t('toolbar.connect')}
		</button>
		<button
			class="toolbar-btn"
			disabled={!active?.connected || workspace.isPending(`disconnect:${active.id}`)}
			onclick={() => active && workspace.disconnect(active.id)}
		>
			<Square size={14} />
			{t('toolbar.disconnect')}
		</button>
		<button
			class="toolbar-btn"
			disabled={!active?.connected}
			onclick={() => active && workspace.askCreateDatabase(active.id)}
		>
			<Database size={14} />
			{t('toolbar.createNoun', {
				noun: active ? schemaNounLabel(active.engine) : t('noun.database')
			})}
		</button>
		<button
			class="toolbar-btn"
			disabled={!active?.connected}
			onclick={() => active && workspace.openQuery(active.id, selected?.schema)}
		>
			<Play size={14} />
			{t('toolbar.newQuery')}
		</button>
		<button
			class="toolbar-btn"
			disabled={!active?.connected}
			onclick={() =>
				selected &&
				workspace.refresh(
					selected.connectionId,
					selected.schema,
					selected.folder,
					selected.objectName
				)}
		>
			<RefreshCw size={14} />
			{t('toolbar.refresh')}
		</button>
		<div class="analysis-menu" bind:this={menuRoot}>
			<button
				class="toolbar-btn"
				type="button"
				aria-haspopup="menu"
				aria-expanded={menuOpen}
				onclick={() => {
					menuOpen = !menuOpen;
					migrationMenuOpen = false;
				}}
			>
				{t('toolbar.analyze')}
				<ChevronDown size={14} />
			</button>
			{#if menuOpen}
				<div class="analysis-menu-pop" role="menu">
					<button type="button" role="menuitem" onclick={openAnalysis}
						>{t('analysis.relationship')}</button
					>
				</div>
			{/if}
		</div>
		<div class="analysis-menu" bind:this={migrationMenuRoot}>
			<button
				class="toolbar-btn"
				type="button"
				aria-haspopup="menu"
				aria-expanded={migrationMenuOpen}
				onclick={() => {
					migrationMenuOpen = !migrationMenuOpen;
					menuOpen = false;
				}}
			>
				{t('toolbar.migrate')}
				<ChevronDown size={14} />
			</button>
			{#if migrationMenuOpen}
				<div class="analysis-menu-pop" role="menu">
					<button type="button" role="menuitem" onclick={openMigration}
						>{t('migration.center')}</button
					>
				</div>
			{/if}
		</div>
		<div class="toolbar-sep"></div>
		<span class="toolbar-status truncate" style="color: var(--text-muted)">
			{#if active}
				<Database size={14} style="display:inline;vertical-align:-2px" />
				{active.name} · {engineLabel(active.engine)} · {active.host}:{active.port}
				{#if active.connected}· {t('toolbar.connected')}{:else}· {t('toolbar.offline')}{/if}
			{:else}
				{t('toolbar.noConnection')}
			{/if}
		</span>
	{:else}
		<div class="toolbar-spacer"></div>
	{/if}

	{#if mcp?.running}
		<span class="mcp-pill" class:on={mcp.enabled}>
			<span class="mcp-dot"></span>
			MCP · {t('agent.port', { port: mcp.port })}
		</span>
	{:else if isTauriRuntime()}
		<span class="mcp-pill">
			<span class="mcp-dot dim"></span>
			{t('agent.starting')}
		</span>
	{/if}

	<button class="toolbar-btn" type="button" onclick={() => (llmSettingsDialog.open = true)}>
		<Settings size={14} />
		{#if appMode.mode === 'database'}{t('toolbar.settings')}{/if}
	</button>
	<div class="locale-switch" role="group" aria-label={t('toolbar.language')}>
		<button
			class="btn"
			class:active={locale === 'en'}
			type="button"
			title="English"
			onclick={() => switchLocale('en')}>{t('toolbar.lang.en')}</button
		>
		<button
			class="btn"
			class:active={locale === 'zh'}
			type="button"
			title="中文"
			onclick={() => switchLocale('zh')}>{t('toolbar.lang.zh')}</button
		>
	</div>
</header>
