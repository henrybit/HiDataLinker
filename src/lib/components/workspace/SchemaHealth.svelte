<script lang="ts">
	import { onMount } from 'svelte';
	import { RefreshCw } from '@lucide/svelte';
	import { api, errorMessage } from '$lib/api/tauri';
	import type { HealthFinding, HealthSeverity, SchemaHealthReport } from '$lib/api/types';
	import { t } from '$lib/i18n/i18n.svelte';
	import { countBySeverity, groupFindings, severityOrder } from '$lib/health/rules';
	import { workspace } from '$lib/stores/workspace.svelte';

	let { connectionId, schema }: { connectionId: string; schema: string } = $props();

	let report = $state<SchemaHealthReport | null>(null);
	let error = $state<string | null>(null);
	let running = $state(false);
	let only = $state<HealthSeverity | undefined>(undefined);

	const counts = $derived(countBySeverity(report?.findings ?? []));
	const groups = $derived(groupFindings(report?.findings ?? [], only));

	async function run() {
		running = true;
		error = null;
		try {
			report = await api.schemaHealth(connectionId, schema);
		} catch (err) {
			error = errorMessage(err);
		} finally {
			running = false;
		}
	}

	function openFix(finding: HealthFinding) {
		if (!finding.fixSql) return;
		void workspace.openQuery(connectionId, schema, finding.fixSql, {
			title: t('health.fixTabTitle', { name: finding.object })
		});
	}

	onMount(run);
</script>

<div class="health">
	<div class="health-toolbar">
		<strong>{t('health.title')}</strong>
		<span class="muted">{schema}</span>
		{#if report}
			<div class="health-filters" role="tablist">
				<button type="button" class:active={only === undefined} onclick={() => (only = undefined)}
					>{t('health.all', { count: report.findings.length })}</button
				>
				{#each severityOrder as severity (severity)}
					<button
						type="button"
						class="sev-{severity}"
						class:active={only === severity}
						disabled={counts[severity] === 0}
						onclick={() => (only = only === severity ? undefined : severity)}
						>{t(`health.severity.${severity}`, { count: counts[severity] })}</button
					>
				{/each}
			</div>
			<span class="muted">
				{t('health.summary', { rules: report.rulesChecked.length, ms: report.durationMs })}
			</span>
		{/if}
		<button type="button" class="toolbar-btn rerun" disabled={running} onclick={run}>
			<RefreshCw size={13} />
			{running ? t('health.running') : t('health.rerun')}
		</button>
	</div>

	<div class="health-note">
		{t('health.fixNote')}
		{#if report}
			{report.statsReset
				? t('health.statsSince', { date: report.statsReset })
				: t('health.statsAlways')}
		{/if}
	</div>

	{#if error}
		<div class="message error">{error}</div>
	{/if}

	<div class="health-body">
		{#if !report}
			{#if !error}<div class="empty">{t('health.running')}</div>{/if}
		{:else if report.findings.length === 0}
			<div class="empty">{t('health.empty')}</div>
		{:else}
			{#each groups as group (group.rule)}
				<section class="health-group">
					<header>
						<span class="badge sev-{group.severity}">{t(`health.badge.${group.severity}`)}</span>
						<strong>{group.meta ? t(group.meta.title) : group.rule}</strong>
						<span class="muted">× {group.findings.length}</span>
						{#if group.meta}
							<span class="source">{t(`health.source.${group.meta.source}`)}</span>
						{/if}
					</header>
					{#if group.meta}<p class="reason">{t(group.meta.reason)}</p>{/if}
					{#each group.findings as finding (finding.object)}
						<div class="finding">
							<div class="finding-head">
								{#if finding.severity !== group.severity}
									<span class="badge sev-{finding.severity}"
										>{t(`health.badge.${finding.severity}`)}</span
									>
								{/if}
								<code class="object">{finding.object}</code>
								{#if finding.facts}<span class="facts">{finding.facts}</span>{/if}
								<span class="spacer"></span>
								{#if finding.fixSql}
									<button type="button" class="toolbar-btn" onclick={() => openFix(finding)}
										>{t('health.openFix')}</button
									>
								{:else}
									<span class="muted">{t('health.manual')}</span>
								{/if}
							</div>
							{#if finding.fixSql}<pre class="fix">{finding.fixSql}</pre>{/if}
						</div>
					{/each}
				</section>
			{/each}
		{/if}
	</div>
</div>

<style>
	.health {
		display: flex;
		flex: 1 1 0;
		min-height: 0;
		flex-direction: column;
	}

	.health-toolbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		padding: 6px 10px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-muted);
		font-size: 12px;
	}

	.health-filters {
		display: flex;
		border: 1px solid var(--border);
		border-radius: 4px;
		overflow: hidden;
	}

	.health-filters button {
		padding: 2px 10px;
		border: 0;
		border-left: 1px solid var(--border);
		background: var(--bg-panel);
		font-size: 12px;
		cursor: pointer;
	}

	.health-filters button:first-child {
		border-left: 0;
	}

	.health-filters button.active {
		background: var(--bg-selected);
	}

	.health-filters button:disabled {
		cursor: default;
		opacity: 0.5;
	}

	.rerun {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-left: auto;
	}

	.health-note {
		padding: 6px 10px;
		border-bottom: 1px solid var(--bg-muted);
		color: var(--text-muted);
		font-size: 12px;
	}

	.health-body {
		flex: 1;
		overflow: auto;
		padding: 8px 10px 16px;
		font-size: 12px;
	}

	.health-group {
		margin-bottom: 12px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg-panel);
	}

	.health-group header {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		padding: 8px 10px 4px;
	}

	.source {
		margin-left: auto;
		color: var(--text-muted);
		font-size: 11px;
	}

	.reason {
		margin: 0;
		padding: 0 10px 8px;
		color: var(--text-muted);
		line-height: 1.5;
	}

	.finding {
		padding: 6px 10px;
		border-top: 1px solid var(--bg-muted);
	}

	.finding-head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.object {
		font-family: var(--mono);
		font-weight: 600;
	}

	.facts {
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11px;
	}

	.spacer {
		flex: 1;
	}

	.fix {
		margin: 6px 0 0;
		padding: 6px 8px;
		border-radius: 4px;
		background: var(--bg-muted);
		font-family: var(--mono);
		font-size: 11px;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.badge {
		padding: 0 6px;
		border-radius: 3px;
		font-size: 11px;
		font-weight: 600;
		line-height: 18px;
	}

	.badge.sev-error {
		background: var(--danger);
		color: #fff;
	}

	.badge.sev-warning {
		background: #fef3c7;
		color: var(--warning);
	}

	.badge.sev-info {
		background: var(--bg-selected);
		color: var(--accent);
	}

	.muted {
		color: var(--text-muted);
	}
</style>
