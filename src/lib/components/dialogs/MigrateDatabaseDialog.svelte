<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { listen, type UnlistenFn } from '@tauri-apps/api/event';
	import type { MigrateProgressEvent } from '$lib/api/types';
	import { normalizeEngine } from '$lib/engine';
	import { schemaNounLabel, schemaNounLower, t } from '$lib/i18n/i18n.svelte';
	import { migrateLevelClass, migratePhaseLabel } from '$lib/migration/labels';
	import { rememberMigration } from '$lib/migration/save';
	import { workspace } from '$lib/stores/workspace.svelte';

	const prompt = $derived(workspace.migrateDatabasePrompt);
	const noun = $derived(schemaNounLabel(prompt?.engine));
	const nounLower = $derived(schemaNounLower(prompt?.engine));
	const pending = $derived(
		prompt ? workspace.isPending(`migrate-db:${prompt.connectionId}:${prompt.name}`) : false
	);

	const sameEngineTargets = $derived(
		prompt
			? workspace.connections.filter(
					(item) =>
						item.connected &&
						normalizeEngine(item.engine) === normalizeEngine(prompt.engine) &&
						!(item.id === prompt.connectionId)
				)
			: []
	);

	let targetConnectionId = $state('');
	let targetName = $state('');
	let includeData = $state(true);
	let logs = $state<MigrateProgressEvent[]>([]);
	let finished = $state(false);
	let logBox = $state<HTMLDivElement | undefined>(undefined);
	let unlisten: UnlistenFn | null = null;

	const targetOptions = $derived.by(() => {
		if (!prompt) return [];
		const options = [...sameEngineTargets];
		const self = workspace.connections.find((item) => item.id === prompt.connectionId);
		if (self?.connected && !options.some((item) => item.id === self.id)) {
			options.unshift(self);
		}
		return options;
	});

	const canSubmit = $derived(
		!!prompt &&
			!!targetConnectionId &&
			targetName.trim().length > 0 &&
			!pending &&
			!finished &&
			!(targetConnectionId === prompt.connectionId && targetName.trim() === prompt.name)
	);

	onMount(() => {
		if (prompt) {
			targetName = prompt.name;
			const first = targetOptions[0];
			targetConnectionId = first?.id ?? '';
		}
		void listen<MigrateProgressEvent>('db-migrate-progress', (event) => {
			logs = [...logs, event.payload];
			if (event.payload.phase === 'done') finished = true;
			queueMicrotask(() => {
				if (logBox) logBox.scrollTop = logBox.scrollHeight;
			});
		}).then((fn) => {
			unlisten = fn;
		});
	});

	onDestroy(() => {
		unlisten?.();
		unlisten = null;
	});

	function cancel() {
		if (pending) return;
		workspace.migrateDatabasePrompt = null;
	}

	function submit(event?: SubmitEvent) {
		event?.preventDefault();
		if (!canSubmit || !prompt) return;
		logs = [];
		finished = false;
		const target = workspace.connections.find((item) => item.id === targetConnectionId);
		const destination = targetName.trim();
		void workspace
			.confirmMigrateDatabase(targetConnectionId, destination, includeData)
			.then(async (result) => {
				const failed = workspace.error;
				if (!failed) finished = true;
				await rememberMigration({
					status: failed ? 'failed' : 'success',
					sourceConnection: prompt.connectionName,
					sourceName: prompt.name,
					targetConnection: target?.name ?? targetConnectionId,
					targetName: destination,
					engine: prompt.engine,
					includeData,
					statementCount: result?.statementCount ?? 0,
					error: failed,
					logs
				});
			});
	}
</script>

{#if prompt}
	<div class="modal-backdrop">
		<div class="modal modal-wide" role="dialog" aria-modal="true">
			<form onsubmit={submit}>
				<header>{t('dialog.migrateNounTitle', { noun })}</header>
				<div class="body">
					<p>
						{t('dialog.migrateNounBody', {
							noun: nounLower,
							name: prompt.name,
							connection: prompt.connectionName
						})}
					</p>

					<label class="field">
						<span>{t('dialog.migrateSource')}</span>
						<input
							type="text"
							readonly
							value="{prompt.connectionName} / {prompt.name}"
							disabled={pending}
						/>
					</label>

					<label class="field">
						<span>{t('dialog.migrateTargetConnection')}</span>
						<select
							bind:value={targetConnectionId}
							disabled={pending || targetOptions.length === 0}
						>
							{#if targetOptions.length === 0}
								<option value="">{t('dialog.migrateNoTargets')}</option>
							{:else}
								{#each targetOptions as item (item.id)}
									<option value={item.id}>
										{item.name}
										{item.id === prompt.connectionId ? t('dialog.migrateSameConnection') : ''}
									</option>
								{/each}
							{/if}
						</select>
					</label>

					<label class="field">
						<span>{t('dialog.migrateTargetName', { noun })}</span>
						<input
							type="text"
							bind:value={targetName}
							disabled={pending}
							placeholder={prompt.name}
						/>
					</label>

					<label class="field">
						<span>{t('dialog.dumpIncludeData')}</span>
						<input type="checkbox" bind:checked={includeData} disabled={pending} />
					</label>

					<p class="hint">{t('dialog.migrateHint', { noun: nounLower })}</p>

					{#if logs.length > 0}
						<div class="migrate-log" bind:this={logBox} role="log" aria-live="polite">
							{#each logs as entry, index (index)}
								<div class="migrate-log-line {migrateLevelClass(entry.level)}">
									<span class="phase">[{migratePhaseLabel(entry.phase)}]</span>
									{#if entry.objectKind && entry.objectName}
										<span class="object">{entry.objectKind}:{entry.objectName}</span>
									{/if}
									{#if entry.total > 0}
										<span class="count">{entry.current}/{entry.total}</span>
									{/if}
									<span class="msg">{entry.message}</span>
								</div>
							{/each}
						</div>
					{/if}
				</div>
				<footer>
					{#if finished}
						<button class="btn primary" type="button" onclick={cancel}>{t('dialog.close')}</button>
					{:else}
						<button class="btn" type="button" onclick={cancel} disabled={pending}
							>{t('dialog.cancel')}</button
						>
						<button class="btn primary" type="submit" disabled={!canSubmit}
							>{pending ? t('dialog.migrating') : t('dialog.migrate')}</button
						>
					{/if}
				</footer>
			</form>
		</div>
	</div>
{/if}
