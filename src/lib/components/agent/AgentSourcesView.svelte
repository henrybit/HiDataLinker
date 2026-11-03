<script lang="ts">
	import Switch from '$lib/components/layout/Switch.svelte';
	import { DEMO_FALLBACK_SOURCES, SEMANTIC_LEARNED } from '$lib/agent/demo';
	import { engineLabel } from '$lib/engine';
	import { t } from '$lib/i18n/i18n.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';

	type Card = {
		id: string;
		engine: string;
		name: string;
		host: string;
		port: number;
		database: string;
		permission: 'rw' | 'ro';
		semanticMappings: number;
		connected: boolean;
	};

	const cards = $derived.by((): Card[] => {
		if (workspace.connections.length > 0) {
			return workspace.connections.map((c) => ({
				id: c.id,
				engine: c.engine,
				name: c.name,
				host: c.host,
				port: c.port,
				database: c.database?.trim() || '—',
				permission: 'rw' as const,
				semanticMappings: 0,
				connected: c.connected
			}));
		}
		return DEMO_FALLBACK_SOURCES;
	});

	function autoOn(id: string): boolean {
		return appMode.prefs.sourceAutoConnect[id] ?? appMode.prefs.autoConnect;
	}
</script>

<div class="agent-sources-page">
	<div class="agent-sources-main">
		<header class="agent-page-head">
			<h2>{t('agentMode.sourcesPage.title')}</h2>
			<p>{t('agentMode.sourcesPage.desc')}</p>
		</header>

		<section>
			<h3 class="agent-section-title">{t('agentMode.sourcesPage.saved')}</h3>
			<div class="agent-source-grid">
				{#each cards as card (card.id)}
					<article class="agent-source-card">
						<div class="agent-source-card-top">
							<span class="agent-engine-tag">{engineLabel(card.engine)}</span>
							<span class="agent-perm-tag" class:ro={card.permission === 'ro'}>
								{card.permission === 'rw'
									? t('agentMode.sourcesPage.rw')
									: t('agentMode.sourcesPage.ro')}
							</span>
						</div>
						<strong class="agent-source-card-name">{card.name}</strong>
						<p class="agent-source-card-status" class:on={card.connected}>
							{card.connected
								? t('agentMode.sourcesPage.connected')
								: t('agentMode.sourcesPage.offline')}
						</p>
						<p class="agent-source-card-dsn truncate">
							{card.host}:{card.port}/{card.database}
						</p>
						<div class="agent-source-card-foot">
							<span
								>{t('agentMode.sourcesPage.semantics', {
									count: card.semanticMappings
								})}</span
							>
							<Switch
								name={card.name}
								checked={autoOn(card.id)}
								onText={t('settings.mcp.on')}
								offText={t('settings.mcp.off')}
								onToggle={(enabled) => appMode.setSourceAutoConnect(card.id, enabled)}
							/>
						</div>
					</article>
				{/each}
			</div>
		</section>

		<section class="agent-diag-row">
			<div class="agent-diag-card">
				<span class="agent-diag-label">{t('agentMode.sourcesPage.perf')}</span>
				<div class="agent-mini-spark" aria-hidden="true">
					<span style="height:40%"></span><span style="height:65%"></span><span
						style="height:50%"
					></span><span style="height:80%"></span><span style="height:55%"></span><span
						style="height:70%"
					></span><span style="height:45%"></span>
				</div>
			</div>
			<div class="agent-diag-card">
				<span class="agent-diag-label">{t('agentMode.sourcesPage.latency')}</span>
				<strong>180 ms</strong>
			</div>
			<div class="agent-diag-card">
				<span class="agent-diag-label">{t('agentMode.sourcesPage.autoCount')}</span>
				<strong>37</strong>
			</div>
			<div class="agent-diag-card warn">
				<span class="agent-diag-label">{t('agentMode.sourcesPage.failCount')}</span>
				<strong>1</strong>
			</div>
		</section>
	</div>

	<aside class="agent-sources-side">
		<section class="agent-context-block">
			<h3>{t('agentMode.behavior.title')}</h3>
			<ul class="agent-toggle-list">
				<li>
					<div>
						<strong>{t('agentMode.behavior.autoConnect')}</strong>
						<p>{t('agentMode.behavior.autoConnectHint')}</p>
					</div>
					<Switch
						name={t('agentMode.behavior.autoConnect')}
						checked={appMode.prefs.autoConnect}
						onText={t('settings.mcp.on')}
						offText={t('settings.mcp.off')}
						onToggle={(v) => appMode.patchPrefs({ autoConnect: v })}
					/>
				</li>
				<li>
					<div>
						<strong>{t('agentMode.behavior.learn')}</strong>
						<p>{t('agentMode.behavior.learnHint')}</p>
					</div>
					<Switch
						name={t('agentMode.behavior.learn')}
						checked={appMode.prefs.autoLearnSemantics}
						onText={t('settings.mcp.on')}
						offText={t('settings.mcp.off')}
						onToggle={(v) => appMode.patchPrefs({ autoLearnSemantics: v })}
					/>
				</li>
				<li>
					<div>
						<strong>{t('agentMode.behavior.cross')}</strong>
						<p>{t('agentMode.behavior.crossHint')}</p>
					</div>
					<Switch
						name={t('agentMode.behavior.cross')}
						checked={appMode.prefs.crossDbJoins}
						onText={t('settings.mcp.on')}
						offText={t('settings.mcp.off')}
						onToggle={(v) => appMode.patchPrefs({ crossDbJoins: v })}
					/>
				</li>
				<li>
					<div>
						<strong>{t('agentMode.behavior.mask')}</strong>
						<p>{t('agentMode.behavior.maskHint')}</p>
					</div>
					<Switch
						name={t('agentMode.behavior.mask')}
						checked={appMode.prefs.maskSensitive}
						onText={t('settings.mcp.on')}
						offText={t('settings.mcp.off')}
						onToggle={(v) => appMode.patchPrefs({ maskSensitive: v })}
					/>
				</li>
				<li>
					<div>
						<strong>{t('agentMode.behavior.writeConfirm')}</strong>
						<p>{t('agentMode.behavior.writeConfirmHint')}</p>
					</div>
					<Switch
						name={t('agentMode.behavior.writeConfirm')}
						checked={appMode.prefs.writeConfirm}
						onText={t('settings.mcp.on')}
						offText={t('settings.mcp.off')}
						onToggle={(v) => appMode.patchPrefs({ writeConfirm: v })}
					/>
				</li>
				<li>
					<div>
						<strong>{t('agentMode.behavior.chartEngine')}</strong>
						<p>{t('agentMode.behavior.chartEngineHint')}</p>
					</div>
					<select
						class="agent-chart-engine-select"
						aria-label={t('agentMode.behavior.chartEngine')}
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
				</li>
			</ul>
		</section>

		<section class="agent-context-block">
			<h3>{t('agentMode.writeAuth.title')}</h3>
			<ul class="agent-auth-list">
				<li>
					<span>{t('agentMode.writeAuth.dev')}</span>
					<em class="ok">{t('agentMode.writeAuth.auto')}</em>
				</li>
				<li>
					<span>acme-billing {t('agentMode.writeAuth.prod')}</span>
					<em class="warn">{t('agentMode.writeAuth.human')}</em>
				</li>
				<li>
					<span>{t('agentMode.writeAuth.otherProd')}</span>
					<em class="bad">{t('agentMode.writeAuth.forbid')}</em>
				</li>
			</ul>
		</section>

		<section class="agent-context-block">
			<h3>{t('agentMode.semantic.title')}</h3>
			<ul class="agent-mapping-list">
				{#each SEMANTIC_LEARNED as row (row.expr)}
					<li>
						<strong>{t(row.termKey)}</strong>
						<code>{row.expr}</code>
					</li>
				{/each}
			</ul>
			<button type="button" class="agent-link-btn" disabled
				>{t('agentMode.semantic.manage')}</button
			>
		</section>
	</aside>
</div>
