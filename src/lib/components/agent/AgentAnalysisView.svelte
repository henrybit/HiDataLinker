<script lang="ts">
	import { FileSpreadsheet, Code2, NotebookPen, Sparkles } from '@lucide/svelte';
	import { ANALYSIS_CHANNELS } from '$lib/agent/demo';
	import { t, type MessageKey } from '$lib/i18n/i18n.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import AgentComposer from './AgentComposer.svelte';

	const userText = $derived(
		appMode.composerDraft.trim() || t('agentMode.analysis.defaultQuery')
	);

	const insights: { n: string; titleKey: MessageKey; bodyKey: MessageKey }[] = [
		{ n: '01', titleKey: 'agentMode.analysis.i1.title', bodyKey: 'agentMode.analysis.i1.body' },
		{ n: '02', titleKey: 'agentMode.analysis.i2.title', bodyKey: 'agentMode.analysis.i2.body' },
		{ n: '03', titleKey: 'agentMode.analysis.i3.title', bodyKey: 'agentMode.analysis.i3.body' }
	];
</script>

<div class="agent-session">
	<header class="agent-session-head">
		<h2>{t('agentMode.analysis.title')}</h2>
	</header>

	<div class="agent-session-scroll">
		<div class="agent-bubble user">{userText}</div>

		<div class="agent-agent-block">
			<div class="agent-agent-label">
				<span class="agent-agent-avatar"><Sparkles size={14} /></span>
				HiDataLinker Agent
			</div>
			<p class="agent-process-line">{t('agentMode.analysis.connecting')}</p>

			<div class="agent-sql-log">
				<div class="agent-sql-card">
					<div class="agent-sql-meta">
						<span>acme-billing · PostgreSQL</span>
						<span>1.2s</span>
					</div>
					<pre>SELECT channel, SUM(gross_amount - refund_amount) AS revenue
FROM billing.orders
WHERE order_month = date_trunc('month', now()) - interval '1 month'
GROUP BY 1;</pre>
				</div>
				<div class="agent-sql-card">
					<div class="agent-sql-meta">
						<span>TiDB Cloud · MySQL</span>
						<span>640ms</span>
					</div>
					<pre>SELECT channel_id, gmv, refund_rate
FROM commerce.order_facts
WHERE dt BETWEEN DATE_SUB(CURDATE(), INTERVAL 60 DAY) AND CURDATE();</pre>
				</div>
			</div>

			<div class="agent-chart-card">
				<div class="agent-chart-title">{t('agentMode.analysis.chartTitle')}</div>
				<ul class="agent-bar-chart">
					{#each ANALYSIS_CHANNELS as row (row.nameKey)}
						<li>
							<span class="agent-bar-name">{t(row.nameKey)}</span>
							<div class="agent-bar-track">
								<div class="agent-bar-fill" style={`width:${row.value}%`}></div>
							</div>
							<span
								class="agent-bar-delta"
								class:down={row.delta.startsWith('-')}>{row.delta}</span
							>
						</li>
					{/each}
				</ul>
			</div>

			<div class="agent-insight-cards">
				{#each insights as item (item.n)}
					<article class="agent-insight-card">
						<span class="agent-insight-n">{item.n}</span>
						<div>
							<strong>{t(item.titleKey)}</strong>
							<p>{t(item.bodyKey)}</p>
						</div>
					</article>
				{/each}
			</div>

			<div class="agent-action-row">
				<button type="button" class="agent-action-btn"><FileSpreadsheet size={14} /> {t('agentMode.analysis.genTable')}</button>
				<button type="button" class="agent-action-btn"><NotebookPen size={14} /> {t('agentMode.analysis.toWeekly')}</button>
				<button type="button" class="agent-action-btn"><Code2 size={14} /> {t('agentMode.analysis.viewSql')}</button>
			</div>
		</div>
	</div>

	<AgentComposer hint={t('agentMode.composer.hintFollow')} />
</div>
