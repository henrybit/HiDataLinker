<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import {
		agentGateway,
		applyAgentStatus,
		pushAgentAudit,
		refreshAgentGateway,
		setAgentGatewayEnabled
	} from '$lib/agent/gateway.svelte';
	import { isTauriRuntime } from '$lib/api/tauri';
	import type { AgentAuditEntry, AgentStatus } from '$lib/api/types';
	import { t } from '$lib/i18n/i18n.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';

	const status = $derived(agentGateway.status);
	const entries = $derived(agentGateway.entries);

	async function toggle(event: Event) {
		const enabled = (event.currentTarget as HTMLInputElement).checked;
		try {
			await setAgentGatewayEnabled(enabled);
		} catch (error) {
			console.error('[agent]', error);
			await refreshAgentGateway();
		}
	}

	onMount(() => {
		if (!isTauriRuntime()) return;
		let cancelled = false;
		const stops: Array<() => void> = [];
		void (async () => {
			await refreshAgentGateway();
			if (cancelled) return;
			stops.push(
				await listen<AgentAuditEntry>('agent-audit', (event) => {
					pushAgentAudit(event.payload);
				})
			);
			stops.push(
				await listen<AgentStatus>('agent-gateway', (event) => {
					applyAgentStatus(event.payload);
				})
			);
			stops.push(
				await listen('agent-session-changed', () => {
					void workspace.reloadConnections();
				})
			);
			if (!agentGateway.status?.running) {
				await new Promise((resolve) => setTimeout(resolve, 400));
				if (!cancelled) await refreshAgentGateway();
			}
		})();
		return () => {
			cancelled = true;
			for (const stop of stops) stop();
		};
	});
</script>

{#if isTauriRuntime()}
	<section class="agent-bar">
		<div class="agent-bar-row">
			<label>
				<input
					type="checkbox"
					checked={status?.enabled ?? false}
					disabled={agentGateway.toggling || !status}
					onchange={toggle}
				/>
				{t('agent.enable')}
			</label>
			<span>
				{#if status?.running}
					{t('agent.port', { port: status.port })}
				{:else}
					{t('agent.starting')}
				{/if}
			</span>
		</div>
		<details>
			<summary>
				{t('agent.recent')}
				{#if entries.length}
					({entries.length})
				{/if}
			</summary>
			{#if entries.length === 0}
				<ul class="agent-log">
					<li>{t('agent.empty')}</li>
				</ul>
			{:else}
				<ul class="agent-log">
					{#each entries as entry (entry.id)}
						<li>
							<span>{entry.at}</span>
							<span>{entry.tool}</span>
							<span class="summary">{entry.summary}</span>
							<span class={entry.ok ? 'ok' : 'fail'}>
								{entry.ok ? t('agent.ok') : t('agent.failed')}
							</span>
							<span>{entry.durationMs}ms</span>
							{#if entry.error}
								<span class="fail summary">{entry.error}</span>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</details>
	</section>
{/if}
