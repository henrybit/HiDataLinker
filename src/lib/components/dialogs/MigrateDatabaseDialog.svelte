<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { listen, type UnlistenFn } from '@tauri-apps/api/event';
	import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
	import type { ConnectionListItem, MigrateProgressEvent } from '$lib/api/types';
	import { testRequestFromConnection } from '$lib/connection-test';
	import { engineLabel, normalizeEngine } from '$lib/engine';
	import { schemaNounLabel, schemaNounLower, t } from '$lib/i18n/i18n.svelte';
	import { migrateLevelClass, migratePhaseLabel } from '$lib/migration/labels';
	import { rememberMigration } from '$lib/migration/save';
	import { workspace } from '$lib/stores/workspace.svelte';

	type CheckState = 'idle' | 'pending' | 'ok' | 'fail';

	const prompt = $derived(workspace.migrateDatabasePrompt);
	const noun = $derived(schemaNounLabel(prompt?.engine));
	const nounLower = $derived(schemaNounLower(prompt?.engine));
	const pending = $derived(
		prompt ? workspace.isPending(`migrate-db:${prompt.connectionId}:${prompt.name}`) : false
	);

	const sameEngineTargets = $derived(
		prompt
			? workspace.connections.filter(
					(item) => normalizeEngine(item.engine) === normalizeEngine(prompt.engine)
				)
			: []
	);

	let targetConnectionId = $state('');
	let targetName = $state('');
	let includeData = $state(true);
	let logs = $state<MigrateProgressEvent[]>([]);
	let finished = $state(false);
	let checking = $state(false);
	let sourceCheck = $state<CheckState>('idle');
	let targetCheck = $state<CheckState>('idle');
	let sourceCheckMessage = $state<string | null>(null);
	let targetCheckMessage = $state<string | null>(null);
	let localError = $state<string | null>(null);
	let logBox = $state<HTMLDivElement | undefined>(undefined);
	let unlisten: UnlistenFn | null = null;

	const targetOptions = $derived.by(() => {
		if (!prompt) return [];
		return sameEngineTargets;
	});

	const canSubmit = $derived(
		!!prompt &&
			!!targetConnectionId &&
			targetName.trim().length > 0 &&
			!pending &&
			!checking &&
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

	function connectionLabel(item: ConnectionListItem): string {
		const status = item.connected ? t('migration.online') : t('migration.offline');
		return `${item.name} · ${engineLabel(item.engine)} · ${status}`;
	}

	function cancel() {
		if (pending || checking) return;
		workspace.migrateDatabasePrompt = null;
	}

	async function probeConnection(
		item: ConnectionListItem
	): Promise<{ ok: true } | { ok: false; message: string }> {
		try {
			await api.testConnection(testRequestFromConnection(item));
			return { ok: true };
		} catch (caught) {
			return { ok: false, message: errorMessage(caught) };
		}
	}

	async function checkEndpoints(
		source: ConnectionListItem,
		target: ConnectionListItem
	): Promise<boolean> {
		sourceCheck = 'pending';
		targetCheck = 'pending';
		sourceCheckMessage = null;
		targetCheckMessage = null;

		const [sourceResult, targetResult] = await Promise.all([
			probeConnection(source),
			source.id === target.id ? Promise.resolve({ ok: true as const }) : probeConnection(target)
		]);

		if (sourceResult.ok) {
			sourceCheck = 'ok';
		} else {
			sourceCheck = 'fail';
			sourceCheckMessage = sourceResult.message;
		}

		if (source.id === target.id) {
			targetCheck = sourceCheck;
			targetCheckMessage = sourceCheckMessage;
		} else if (targetResult.ok) {
			targetCheck = 'ok';
		} else {
			targetCheck = 'fail';
			targetCheckMessage = 'message' in targetResult ? targetResult.message : null;
		}

		return sourceCheck === 'ok' && targetCheck === 'ok';
	}

	function checkLabel(state: CheckState): string {
		if (state === 'pending') return t('migration.checkPending');
		if (state === 'ok') return t('migration.checkOk');
		if (state === 'fail') return t('migration.checkFail');
		return '';
	}

	async function submit(event?: SubmitEvent) {
		event?.preventDefault();
		if (!canSubmit || !prompt) return;
		if (!isTauriRuntime()) {
			localError = t('migration.needTauri');
			return;
		}

		const source = workspace.connections.find((item) => item.id === prompt.connectionId);
		const target = workspace.connections.find((item) => item.id === targetConnectionId);
		if (!source || !target) {
			localError = t('migration.needTarget');
			return;
		}

		logs = [];
		finished = false;
		localError = null;
		checking = true;
		const destination = targetName.trim();

		try {
			const reachable = await checkEndpoints(source, target);
			if (!reachable) {
				localError = t('dialog.migrateCheckFailed');
				if (sourceCheckMessage) {
					localError = `${localError}\n${t('migration.checkSource')}: ${sourceCheckMessage}`;
				}
				if (targetCheckMessage && source.id !== target.id) {
					localError = `${localError}\n${t('migration.checkTarget')}: ${targetCheckMessage}`;
				}
				return;
			}

			if (!source.connected) {
				const ok = await workspace.connectAsync(source.id);
				if (!ok) {
					localError = t('migration.connectFailed', { name: source.name });
					return;
				}
			}
			if (!target.connected) {
				const ok = await workspace.connectAsync(target.id);
				if (!ok) {
					localError = t('migration.connectFailed', { name: target.name });
					return;
				}
			}

			checking = false;
			const result = await workspace.confirmMigrateDatabase(
				targetConnectionId,
				destination,
				includeData
			);
			const failed = workspace.error;
			if (!failed) finished = true;
			await rememberMigration({
				status: failed ? 'failed' : 'success',
				sourceConnection: prompt.connectionName,
				sourceName: prompt.name,
				targetConnection: target.name,
				targetName: destination,
				engine: prompt.engine,
				includeData,
				statementCount: result?.statementCount ?? 0,
				error: failed,
				logs
			});
		} finally {
			checking = false;
		}
	}
</script>

{#if prompt}
	<div class="modal-backdrop">
		<div class="modal modal-wide" role="dialog" aria-modal="true">
			<form onsubmit={(event) => void submit(event)}>
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
							disabled={pending || checking}
						/>
					</label>

					<label class="field">
						<span>{t('dialog.migrateTargetConnection')}</span>
						<select
							bind:value={targetConnectionId}
							disabled={pending || checking || targetOptions.length === 0}
						>
							{#if targetOptions.length === 0}
								<option value="">{t('dialog.migrateNoTargets')}</option>
							{:else}
								{#each targetOptions as item (item.id)}
									<option value={item.id}>
										{connectionLabel(item)}{item.id === prompt.connectionId
											? t('dialog.migrateSameConnection')
											: ''}
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
							disabled={pending || checking}
							placeholder={prompt.name}
						/>
					</label>

					<label class="field">
						<span>{t('dialog.dumpIncludeData')}</span>
						<input type="checkbox" bind:checked={includeData} disabled={pending || checking} />
					</label>

					{#if sourceCheck !== 'idle' || targetCheck !== 'idle'}
						<div class="migration-checks" aria-live="polite">
							<p class="migration-check {sourceCheck}">
								{t('migration.checkSource')}: {checkLabel(sourceCheck)}
								{#if sourceCheckMessage}
									<span class="migration-check-detail">{sourceCheckMessage}</span>
								{/if}
							</p>
							<p class="migration-check {targetCheck}">
								{t('migration.checkTarget')}: {checkLabel(targetCheck)}
								{#if targetCheckMessage && prompt.connectionId !== targetConnectionId}
									<span class="migration-check-detail">{targetCheckMessage}</span>
								{/if}
							</p>
						</div>
					{/if}

					<p class="hint">{t('dialog.migrateHint', { noun: nounLower })}</p>

					{#if localError}
						<p class="message error">{localError}</p>
					{/if}

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
						<button class="btn" type="button" onclick={cancel} disabled={pending || checking}
							>{t('dialog.cancel')}</button
						>
						<button class="btn primary" type="submit" disabled={!canSubmit}
							>{checking
								? t('dialog.migrateChecking')
								: pending
									? t('dialog.migrating')
									: t('dialog.migrate')}</button
						>
					{/if}
				</footer>
			</form>
		</div>
	</div>
{/if}
