<script lang="ts">
	import { onMount } from 'svelte';
	import { Check, LoaderCircle, MessageSquare, RefreshCw, Square } from '@lucide/svelte';
	import type { InsightCategory, InsightSeverity } from '$lib/api/types';
	import { localeTag, t, type MessageKey } from '$lib/i18n/i18n.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import { insights } from '$lib/stores/insights.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';

	const sevKey: Record<InsightSeverity, MessageKey> = {
		high: 'agentMode.sev.high',
		medium: 'agentMode.sev.medium',
		low: 'agentMode.sev.low'
	};

	const catKey: Record<InsightCategory, MessageKey> = {
		business: 'agentMode.insights.cat.business',
		quality: 'agentMode.insights.cat.quality',
		performance: 'agentMode.insights.cat.performance',
		governance: 'agentMode.insights.cat.governance',
		integration: 'agentMode.insights.cat.integration'
	};

	onMount(() => {
		void insights.init();
	});

	const items = $derived(insights.report?.items ?? []);

	const stats = $derived.by(() => {
		const count = (sev: InsightSeverity) => items.filter((i) => i.severity === sev).length;
		const processed = items.filter((i) => appMode.processedInsightIds.has(i.id)).length;
		return {
			high: count('high'),
			medium: count('medium'),
			low: count('low'),
			processed,
			pending: items.length - processed
		};
	});

	const generatedAt = $derived.by(() => {
		const raw = insights.report?.generatedAt;
		if (!raw) return null;
		const at = new Date(raw);
		return Number.isNaN(at.getTime()) ? null : at.toLocaleString(localeTag());
	});

	function connectionName(id: string): string {
		return workspace.connections.find((conn) => conn.id === id)?.name ?? id;
	}
</script>

<div class="agent-insights-page">
	<div class="agent-insights-main">
		<header class="agent-page-head agent-insights-head">
			<div>
				<h2>{t('agentMode.insights.title')}</h2>
				<p>{t('agentMode.insights.desc')}</p>
			</div>
			<div class="agent-insights-toolbar">
				{#if generatedAt && !insights.running}
					<span class="agent-insights-time">
						{t('agentMode.insights.generatedAt', { time: generatedAt })}
					</span>
				{/if}
				{#if insights.running}
					<button type="button" class="agent-action-btn" onclick={() => insights.cancel()}>
						<Square size={14} />
						{t('agentMode.insights.cancel')}
					</button>
				{:else}
					<button
						type="button"
						class="agent-action-btn primary"
						disabled={workspace.connections.length === 0}
						onclick={() => insights.run()}
					>
						<RefreshCw size={14} />
						{t('agentMode.insights.refresh')}
					</button>
				{/if}
			</div>
		</header>

		{#if insights.running}
			<div class="agent-insights-status">
				<LoaderCircle size={14} class="agent-spin" />
				<span>{insights.statusMessage ?? t('agentMode.insights.starting')}</span>
				<small>{t('agentMode.insights.runningHint')}</small>
			</div>
		{/if}

		{#if insights.error}
			<p class="agent-insights-error">{insights.error}</p>
		{/if}

		{#if items.length === 0}
			{#if insights.loaded && !insights.running}
				<p class="agent-insights-empty">
					{workspace.connections.length === 0
						? t('agentMode.insights.emptyNoConn')
						: t('agentMode.insights.empty')}
				</p>
			{/if}
		{:else}
			<ul class="agent-insight-list">
				{#each items as item (item.id)}
					{@const handled = appMode.processedInsightIds.has(item.id)}
					<li class="agent-insight-row" class:handled>
						<div class="agent-insight-body">
							<div class="agent-insight-title-row">
								<strong>{item.title}</strong>
								<span
									class="agent-sev"
									class:high={item.severity === 'high'}
									class:medium={item.severity === 'medium'}
									class:low={item.severity === 'low'}
								>
									{t(sevKey[item.severity])}
								</span>
								<span class="agent-insight-cat">{t(catKey[item.category])}</span>
							</div>
							<p>{item.body}</p>
							<div class="agent-insight-tags">
								{#each item.connectionIds as id (id)}
									<span class="src">{connectionName(id)}</span>
								{/each}
								{#each item.tables as table (table)}
									<span class="metric mono">{table}</span>
								{/each}
								{#each item.tags as tag (tag)}
									<span class="metric">{tag}</span>
								{/each}
							</div>
						</div>
						<div class="agent-insight-actions">
							<button
								type="button"
								class="agent-action-btn primary"
								title={item.prompt}
								onclick={() => insights.analyzeInChat(item)}
							>
								<MessageSquare size={14} />
								{t('agentMode.insights.analyzeInChat')}
							</button>
							<button
								type="button"
								class="agent-action-btn"
								disabled={handled}
								onclick={() => appMode.convertInsightToTask(item.id)}
							>
								<Check size={14} />
								{handled ? t('agentMode.insights.handled') : t('agentMode.insights.markDone')}
							</button>
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	<aside class="agent-insights-side">
		<section class="agent-context-block">
			<h3>{t('agentMode.insights.profile')}</h3>
			{#if insights.report?.connections.length}
				<ul class="agent-insights-profiles">
					{#each insights.report.connections as conn (conn.id)}
						<li>
							<div>
								<strong>{connectionName(conn.id)}</strong>
								<span class="agent-insight-cat">{conn.domain}</span>
							</div>
							<p>{conn.summary}</p>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="agent-insights-muted">{t('agentMode.insights.profileEmpty')}</p>
			{/if}
		</section>

		<section class="agent-context-block">
			<h3>{t('agentMode.insights.stats')}</h3>
			<div class="agent-stat-sev">
				<span class="high">{stats.high} {t('agentMode.sev.high')}</span>
				<span class="medium">{stats.medium} {t('agentMode.sev.medium')}</span>
				<span class="low">{stats.low} {t('agentMode.sev.low')}</span>
			</div>
			<dl class="agent-kv">
				<div>
					<dt>{t('agentMode.insights.processed')}</dt>
					<dd>{stats.processed}</dd>
				</div>
				<div>
					<dt>{t('agentMode.insights.pending')}</dt>
					<dd>{stats.pending}</dd>
				</div>
			</dl>
		</section>
	</aside>
</div>
