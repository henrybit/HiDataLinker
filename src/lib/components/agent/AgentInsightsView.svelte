<script lang="ts">
	import { ListTodo, Search } from '@lucide/svelte';
	import { DEMO_INSIGHTS } from '$lib/agent/demo';
	import { t, type MessageKey } from '$lib/i18n/i18n.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';

	const sevKey: Record<'high' | 'medium' | 'low', MessageKey> = {
		high: 'agentMode.sev.high',
		medium: 'agentMode.sev.medium',
		low: 'agentMode.sev.low'
	};

	const stats = $derived.by(() => {
		const high = DEMO_INSIGHTS.filter((i) => i.severity === 'high').length;
		const medium = DEMO_INSIGHTS.filter((i) => i.severity === 'medium').length;
		const low = DEMO_INSIGHTS.filter((i) => i.severity === 'low').length;
		const processed = appMode.processedInsightIds.size;
		const autoTasks = appMode.taskedInsightIds.size;
		const pending = Math.max(0, DEMO_INSIGHTS.length - processed);
		return { high, medium, low, processed: 7 + processed, pending, autoTasks: 3 + autoTasks };
	});
</script>

<div class="agent-insights-page">
	<div class="agent-insights-main">
		<header class="agent-page-head">
			<h2>{t('agentMode.insights.title')}</h2>
			<p>{t('agentMode.insights.desc')}</p>
		</header>

		<ul class="agent-insight-list">
			{#each DEMO_INSIGHTS as item (item.id)}
				<li class="agent-insight-row">
					<div class="agent-insight-body">
						<div class="agent-insight-title-row">
							<strong>{t(item.titleKey)}</strong>
							<span
								class="agent-sev"
								class:high={item.severity === 'high'}
								class:medium={item.severity === 'medium'}
								class:low={item.severity === 'low'}
							>
								{t(sevKey[item.severity])}
							</span>
						</div>
						<p>{t(item.bodyKey)}</p>
						<div class="agent-insight-tags">
							{#each item.sources as src}
								<span class="src">{src}</span>
							{/each}
							{#each item.metrics as m}
								<span class="metric">{m}</span>
							{/each}
						</div>
					</div>
					<div class="agent-insight-actions">
						<button type="button" class="agent-action-btn" disabled>
							<Search size={14} /> {t('agentMode.insights.evidence')}
						</button>
						<button
							type="button"
							class="agent-action-btn primary"
							disabled={appMode.taskedInsightIds.has(item.id)}
							onclick={() => appMode.convertInsightToTask(item.id)}
						>
							<ListTodo size={14} />
							{appMode.taskedInsightIds.has(item.id)
								? t('agentMode.insights.tasked')
								: t('agentMode.insights.toTask')}
						</button>
					</div>
				</li>
			{/each}
		</ul>
	</div>

	<aside class="agent-insights-side">
		<section class="agent-context-block">
			<div class="agent-side-head">
				<h3>{t('agentMode.insights.patrol')}</h3>
				<button type="button" class="agent-link-btn" disabled
					>+ {t('agentMode.insights.newPatrol')}</button
				>
			</div>
			<ul class="agent-patrol-list">
				<li>
					<strong>{t('agentMode.insights.daily')}</strong>
					<span>08:00</span>
				</li>
				<li>
					<strong>{t('agentMode.insights.weekly')}</strong>
					<span>{t('agentMode.insights.monday')} 09:00</span>
				</li>
			</ul>
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
				<div>
					<dt>{t('agentMode.insights.autoTasks')}</dt>
					<dd>{stats.autoTasks}</dd>
				</div>
			</dl>
		</section>
	</aside>
</div>
