<script lang="ts">
	import { Sparkles } from '@lucide/svelte';
	import { WRITE_PREVIEW_ROWS } from '$lib/agent/demo';
	import { t } from '$lib/i18n/i18n.svelte';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import { agentChat } from '$lib/stores/agent-chat.svelte';
	import AgentComposer from './AgentComposer.svelte';

	const userText = $derived(
		appMode.composerDraft.trim() || t('agentMode.write.defaultQuery')
	);

	function confirmWrite() {
		appMode.setWritePlanStatus('confirmed', 'agentMode.write.demoConfirmed');
	}

	function dryRun() {
		appMode.setWritePlanStatus('dry-run', 'agentMode.write.demoDryRun');
	}

	function cancelWrite() {
		appMode.setWritePlanStatus('canceled', 'agentMode.write.demoCanceled');
	}
</script>

<div class="agent-session">
	<header class="agent-session-head">
		<h2>{t('agentMode.write.title')}</h2>
		{#if appMode.statusMessage}
			<span class="agent-status-toast">{t(appMode.statusMessage)}</span>
		{/if}
	</header>

	<div class="agent-session-scroll">
		<div class="agent-bubble user">{userText}</div>

		<div class="agent-agent-block">
			<div class="agent-agent-label">
				<span class="agent-agent-avatar"><Sparkles size={14} /></span>
				HiDataLinker Agent
			</div>
			<p class="agent-process-line">{t('agentMode.write.dryRunNote')}</p>

			<div class="agent-sql-card">
				<div class="agent-sql-meta">
					<span>acme-billing · PostgreSQL · dry-run</span>
					<span>180ms</span>
				</div>
				<pre>SELECT count(*) FROM billing.orders
WHERE order_month = date_trunc('month', now()) - interval '1 month'
  AND refund_rate > 0.08;</pre>
			</div>

			<div class="agent-write-plan" class:done={appMode.writePlanStatus === 'confirmed'}>
				<div class="agent-write-plan-head">
					<strong>{t('agentMode.write.planTitle')}</strong>
					<span class="agent-prod-tag">{t('agentMode.write.prod')}</span>
				</div>
				<div class="agent-write-stats">
					<div>
						<span>{t('agentMode.write.impactRows')}</span>
						<strong>1,284</strong>
					</div>
					<div>
						<span>{t('agentMode.write.table')}</span>
						<strong>billing.orders</strong>
					</div>
					<div>
						<span>{t('agentMode.write.target')}</span>
						<strong>acme-billing</strong>
					</div>
					<div>
						<span>{t('agentMode.write.txn')}</span>
						<strong>{t('agentMode.write.txnSingle')}</strong>
					</div>
				</div>

				<pre class="agent-write-sql">UPDATE billing.orders
SET review_status = 'Need Review',
    updated_at = now()
WHERE order_month = date_trunc('month', now()) - interval '1 month'
  AND refund_rate > 0.08
  AND review_status = 'Pending';</pre>

				<div class="agent-preview-table-wrap">
					<table class="agent-preview-table">
						<thead>
							<tr>
								<th>{t('agentMode.write.colOrder')}</th>
								<th>{t('agentMode.write.colRefund')}</th>
								<th>{t('agentMode.write.colChannel')}</th>
								<th>{t('agentMode.write.colChange')}</th>
							</tr>
						</thead>
						<tbody>
							{#each WRITE_PREVIEW_ROWS as row (row.orderId)}
								<tr>
									<td>{row.orderId}</td>
									<td class="danger">{row.refundRate}</td>
									<td>{row.channel}</td>
									<td>{row.change}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>

				{#if appMode.writePlanStatus === 'pending' || appMode.writePlanStatus === 'dry-run'}
					<div class="agent-write-actions">
						<button type="button" class="agent-btn primary" onclick={confirmWrite}>
							{t('agentMode.write.confirm')}
						</button>
						<button type="button" class="agent-btn" onclick={dryRun}>
							{t('agentMode.write.dryRunOnly')}
						</button>
						<button type="button" class="agent-btn ghost" onclick={cancelWrite}>
							{t('agentMode.write.cancel')}
						</button>
					</div>
				{/if}
			</div>
		</div>
	</div>

	<div class="agent-follow-chips">
		<button
			type="button"
			class="agent-suggest-chip"
			onclick={() => (appMode.composerDraft = t('agentMode.write.follow12'))}
		>
			{t('agentMode.write.follow12')}
		</button>
		<button
			type="button"
			class="agent-suggest-chip"
			onclick={() => (appMode.composerDraft = t('agentMode.write.followChannel'))}
		>
			{t('agentMode.write.followChannel')}
		</button>
	</div>
	<AgentComposer
		hint={t('agentMode.composer.hintWrite')}
		seedText={appMode.composerDraft}
		onSubmit={(text, connectionIds, skillIds) => {
			appMode.openHome();
			void agentChat.newChat().then(() => agentChat.submit(text, connectionIds, skillIds));
		}}
	/>
</div>
