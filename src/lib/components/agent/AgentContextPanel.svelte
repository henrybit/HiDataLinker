<script lang="ts">
	import {
		ANALYSIS_MAPPINGS,
		ANALYSIS_STEPS,
		ANALYSIS_TABLES,
		WRITE_IMPACT
	} from '$lib/agent/demo';
	import { t } from '$lib/i18n/i18n.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';

	const kind = $derived(appMode.sessionId === 'write-orders' ? 'write' : 'analysis');
</script>

<aside class="agent-context">
	{#if kind === 'analysis'}
		<section class="agent-context-block">
			<h3>{t('agentMode.context.active')}</h3>
			<ul class="agent-pill-list">
				<li>acme-billing</li>
				<li>TiDB Cloud</li>
				<li>Uniope-Sales-Prod</li>
			</ul>
		</section>
		<section class="agent-context-block">
			<h3>{t('agentMode.context.mapping')}</h3>
			<ul class="agent-mapping-list">
				{#each ANALYSIS_MAPPINGS as row (row.expr)}
					<li>
						<strong>{t(row.termKey)}</strong>
						<code>{row.expr}</code>
					</li>
				{/each}
			</ul>
		</section>
		<section class="agent-context-block">
			<h3>{t('agentMode.context.tables')}</h3>
			<ul class="agent-table-refs">
				{#each ANALYSIS_TABLES as row (row.name)}
					<li>
						<code>{row.name}</code>
						<span>{row.rows}</span>
					</li>
				{/each}
			</ul>
		</section>
		<section class="agent-context-block">
			<h3>{t('agentMode.context.trace')}</h3>
			<ol class="agent-trace">
				{#each ANALYSIS_STEPS as step (step.id)}
					<li>
						<span class="agent-trace-n">{step.id}</span>
						<div>
							<strong>{t(step.titleKey)}</strong>
							<p>{t(step.detailKey)}</p>
						</div>
					</li>
				{/each}
			</ol>
		</section>
	{:else}
		<section class="agent-context-block">
			<h3>{t('agentMode.context.impact')}</h3>
			<ul class="agent-impact-bars">
				{#each WRITE_IMPACT as row (row.channel)}
					<li>
						<div class="agent-impact-label">
							<span>{row.channel}</span>
							<span>{row.rows}</span>
						</div>
						<div class="agent-impact-track">
							<div class="agent-impact-fill" style={`width:${row.pct}%`}></div>
						</div>
					</li>
				{/each}
			</ul>
		</section>
		<section class="agent-context-block">
			<h3>{t('agentMode.context.strategy')}</h3>
			<dl class="agent-kv">
				<div><dt>{t('agentMode.write.txn')}</dt><dd>{t('agentMode.write.txnSingle')}</dd></div>
				<div><dt>{t('agentMode.write.batch')}</dt><dd>500</dd></div>
				<div><dt>{t('agentMode.write.timeout')}</dt><dd>30s</dd></div>
				<div><dt>{t('agentMode.write.failure')}</dt><dd>{t('agentMode.write.rollback')}</dd></div>
			</dl>
		</section>
		<section class="agent-context-block">
			<h3>{t('agentMode.context.audit')}</h3>
			<dl class="agent-kv">
				<div><dt>{t('agentMode.write.account')}</dt><dd>hdl_ro_write</dd></div>
				<div><dt>{t('agentMode.write.approval')}</dt><dd>{t('agentMode.write.humanConfirm')}</dd></div>
				<div><dt>{t('agentMode.write.window')}</dt><dd>7d</dd></div>
				<div><dt>{t('agentMode.write.predicted')}</dt><dd>1,284</dd></div>
			</dl>
		</section>
	{/if}
</aside>
