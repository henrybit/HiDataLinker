<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { listen, type UnlistenFn } from '@tauri-apps/api/event';
	import { X } from '@lucide/svelte';
	import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
	import type { ConnectionListItem, MigrateProgressEvent } from '$lib/api/types';
	import { testRequestFromConnection } from '$lib/connection-test';
	import { engineLabel, normalizeEngine } from '$lib/engine';
	import { schemaNounLabel, schemaNounLower, t } from '$lib/i18n/i18n.svelte';
	import { asMigrationLogs, type MigrationHistorySummary } from '$lib/migration/history';
	import {
		migrateLevelClass,
		migratePhaseLabel,
		migrationStatusLabel
	} from '$lib/migration/labels';
	import { migrationPanel } from '$lib/migration/panel.svelte';
	import { rememberMigration } from '$lib/migration/save';
	import { workspace } from '$lib/stores/workspace.svelte';

	type CheckState = 'idle' | 'pending' | 'ok' | 'fail';

	let history = $state<MigrationHistorySummary[]>([]);
	let viewingId = $state<string | null>(null);
	const viewing = $derived(history.find((item) => item.id === viewingId) ?? null);
	let sourceConnectionId = $state('');
	let sourceName = $state('');
	let targetConnectionId = $state('');
	let targetName = $state('');
	let includeData = $state(true);
	let running = $state(false);
	let finished = $state(false);
	let error = $state<string | null>(null);
	let logs = $state<MigrateProgressEvent[]>([]);
	let logBox = $state<HTMLDivElement | undefined>(undefined);
	let unlisten: UnlistenFn | null = null;
	let loadingSchemas = $state(false);
	let connectingSource = $state(false);
	let sourceCheck = $state<CheckState>('idle');
	let targetCheck = $state<CheckState>('idle');
	let sourceCheckMessage = $state<string | null>(null);
	let targetCheckMessage = $state<string | null>(null);
	let awaitingNewConnection = $state(false);
	let knownConnectionIds = $state<Set<string>>(new Set());

	const sourceConnection = $derived(
		workspace.connections.find((item) => item.id === sourceConnectionId) ?? null
	);
	const savedConnections = $derived(workspace.connections);
	const sourceSchemas = $derived(
		sourceConnectionId ? (workspace.schema[sourceConnectionId]?.databases ?? []) : []
	);
	const targetOptions = $derived.by(() => {
		if (!sourceConnection) return [];
		const engine = normalizeEngine(sourceConnection.engine);
		return workspace.connections.filter((item) => normalizeEngine(item.engine) === engine);
	});
	const noun = $derived(schemaNounLabel(sourceConnection?.engine));
	const nounLower = $derived(schemaNounLower(sourceConnection?.engine));
	const checking = $derived(sourceCheck === 'pending' || targetCheck === 'pending');
	const canSubmit = $derived(
		!!sourceConnectionId &&
			!!sourceName.trim() &&
			!!targetConnectionId &&
			!!targetName.trim() &&
			!running &&
			!checking &&
			!connectingSource &&
			!(sourceConnectionId === targetConnectionId && sourceName.trim() === targetName.trim())
	);

	onMount(() => {
		void loadHistory();
		const first = savedConnections[0];
		if (first) {
			sourceConnectionId = first.id;
			targetConnectionId = first.id;
			void prepareSource(first.id);
		}
		void listen<MigrateProgressEvent>('db-migrate-progress', (event) => {
			if (!running) return;
			logs = [...logs, event.payload];
			if (event.payload.phase === 'done') finished = true;
			queueMicrotask(() => {
				if (logBox) logBox.scrollTop = logBox.scrollHeight;
			});
		}).then((fn) => {
			unlisten = fn;
		});
		const onKey = (event: KeyboardEvent) => {
			if (event.key !== 'Escape' || event.defaultPrevented || running) return;
			if (workspace.dialogOpen || workspace.passwordPrompt) return;
			migrationPanel.open = false;
		};
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
	});

	onDestroy(() => {
		unlisten?.();
		unlisten = null;
	});

	$effect(() => {
		if (!awaitingNewConnection || workspace.dialogOpen) return;
		const added = workspace.connections.find((item) => !knownConnectionIds.has(item.id));
		awaitingNewConnection = false;
		if (!added) return;
		void onSourceConnectionChange(added.id);
	});

	$effect(() => {
		if (!sourceConnectionId || connectingSource || loadingSchemas) return;
		const connection = workspace.connections.find((item) => item.id === sourceConnectionId);
		if (!connection?.connected) return;
		if ((workspace.schema[sourceConnectionId]?.databases.length ?? 0) > 0) return;
		void ensureSourceSchemas(sourceConnectionId);
	});

	async function loadHistory() {
		if (!isTauriRuntime()) {
			history = [];
			return;
		}
		try {
			history = await api.listMigrationHistory();
		} catch (caught) {
			error = errorMessage(caught);
		}
	}

	function connectionLabel(item: ConnectionListItem): string {
		const status = item.connected ? t('migration.online') : t('migration.offline');
		return `${item.name} · ${engineLabel(item.engine)} · ${status}`;
	}

	function resetChecks() {
		sourceCheck = 'idle';
		targetCheck = 'idle';
		sourceCheckMessage = null;
		targetCheckMessage = null;
	}

	async function ensureLiveSession(connectionId: string): Promise<boolean> {
		const connection = workspace.connections.find((item) => item.id === connectionId);
		if (!connection) return false;
		if (connection.connected) return true;
		return workspace.connectAsync(connectionId);
	}

	async function ensureSourceSchemas(connectionId: string) {
		if (!connectionId) return;
		const connection = workspace.connections.find((item) => item.id === connectionId);
		if (!connection?.connected) return;
		if ((workspace.schema[connectionId]?.databases.length ?? 0) > 0) return;
		loadingSchemas = true;
		try {
			await workspace.loadDatabases(connectionId);
		} finally {
			loadingSchemas = false;
		}
	}

	async function prepareSource(connectionId: string) {
		if (!connectionId) return;
		connectingSource = true;
		error = null;
		try {
			const connected = await ensureLiveSession(connectionId);
			if (!connected) {
				error = t('migration.connectFailed', {
					name:
						workspace.connections.find((item) => item.id === connectionId)?.name ?? connectionId
				});
				return;
			}
			await ensureSourceSchemas(connectionId);
		} finally {
			connectingSource = false;
		}
	}

	async function onSourceConnectionChange(connectionId: string) {
		sourceConnectionId = connectionId;
		sourceName = '';
		targetName = '';
		resetChecks();
		const engine = normalizeEngine(
			workspace.connections.find((item) => item.id === connectionId)?.engine
		);
		const options = workspace.connections.filter((item) => normalizeEngine(item.engine) === engine);
		if (!options.some((item) => item.id === targetConnectionId)) {
			targetConnectionId = options[0]?.id ?? '';
		}
		await prepareSource(connectionId);
	}

	async function onSourceNameChange(name: string) {
		sourceName = name;
		if (!targetName.trim() || targetName === sourceName) {
			targetName = name;
		}
	}

	function onTargetConnectionChange(connectionId: string) {
		targetConnectionId = connectionId;
		resetChecks();
	}

	function openNewConnection() {
		knownConnectionIds = new Set(workspace.connections.map((item) => item.id));
		awaitingNewConnection = true;
		workspace.openNewConnection();
	}

	function startNewTask() {
		viewingId = null;
		logs = [];
		finished = false;
		error = null;
		resetChecks();
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
		if (!canSubmit || !sourceConnection) return;
		if (!isTauriRuntime()) {
			error = t('migration.needTauri');
			return;
		}
		const target = workspace.connections.find((item) => item.id === targetConnectionId);
		if (!target) {
			error = t('migration.needTarget');
			return;
		}

		viewingId = null;
		finished = false;
		error = null;
		logs = [];
		const sourceLabel = sourceConnection.name;
		const targetLabel = target.name;
		const source = sourceName.trim();
		const destination = targetName.trim();

		running = true;
		let statementCount = 0;
		let failed: string | null = null;
		let startedMigrate = false;
		try {
			const reachable = await checkEndpoints(sourceConnection, target);
			if (!reachable) {
				error = t('dialog.migrateCheckFailed');
				if (sourceCheckMessage) {
					error = `${error}\n${t('migration.checkSource')}: ${sourceCheckMessage}`;
				}
				if (targetCheckMessage && sourceConnection.id !== target.id) {
					error = `${error}\n${t('migration.checkTarget')}: ${targetCheckMessage}`;
				}
				return;
			}

			const sourceReady = await ensureLiveSession(sourceConnectionId);
			if (!sourceReady) {
				error = t('migration.connectFailed', { name: sourceLabel });
				return;
			}
			const targetReady = await ensureLiveSession(targetConnectionId);
			if (!targetReady) {
				error = t('migration.connectFailed', { name: targetLabel });
				return;
			}

			startedMigrate = true;
			const result = await api.migrateDatabase(
				sourceConnectionId,
				source,
				targetConnectionId,
				destination,
				includeData
			);
			statementCount = result.statementCount;
			await workspace.loadDatabases(targetConnectionId);
			workspace.expanded = new Set([...workspace.expanded, `conn:${targetConnectionId}`]);
			workspace.status = t('status.migrated', {
				noun,
				source: result.sourceName,
				target: result.targetName,
				count: result.statementCount
			});
			finished = true;
		} catch (caught) {
			failed = errorMessage(caught);
			error = failed;
			workspace.error = failed;
		} finally {
			running = false;
			if (!startedMigrate) return;
			const remembered = await rememberMigration({
				status: failed ? 'failed' : 'success',
				sourceConnection: sourceLabel,
				sourceName: source,
				targetConnection: targetLabel,
				targetName: destination,
				engine: sourceConnection.engine,
				includeData,
				statementCount,
				error: failed,
				logs
			});
			if (remembered.saved) {
				history = [remembered.saved, ...history.filter((item) => item.id !== remembered.saved!.id)];
				viewingId = remembered.saved.id;
			} else if (remembered.warning) {
				error = error ? `${error}\n${remembered.warning}` : remembered.warning;
			}
		}
	}

	async function openHistory(id: string) {
		if (running) return;
		try {
			const record = await api.readMigrationHistory(id);
			viewingId = record.id;
			logs = asMigrationLogs(record.logs);
			finished = true;
			error = record.error ?? null;
			sourceConnectionId =
				workspace.connections.find((item) => item.name === record.sourceConnection)?.id ??
				sourceConnectionId;
			sourceName = record.sourceName;
			targetConnectionId =
				workspace.connections.find((item) => item.name === record.targetConnection)?.id ??
				targetConnectionId;
			targetName = record.targetName;
			includeData = record.includeData;
			resetChecks();
		} catch (caught) {
			error = errorMessage(caught);
		}
	}

	async function deleteHistory(id: string) {
		if (running) return;
		try {
			await api.deleteMigrationHistory(id);
			history = history.filter((item) => item.id !== id);
			if (viewingId === id) {
				viewingId = null;
				logs = [];
				finished = false;
				error = null;
			}
		} catch (caught) {
			error = errorMessage(caught);
		}
	}

	function historyTime(value: string): string {
		const date = new Date(value);
		if (Number.isNaN(date.getTime())) return value;
		return date.toLocaleString();
	}
</script>

<div class="modal-backdrop">
	<div class="modal analysis-panel migration-panel" role="dialog" aria-labelledby="migration-title">
		<header>
			<span id="migration-title">{t('migration.title')}</span>
			<div class="analysis-header-actions">
				<button class="btn" type="button" onclick={startNewTask} disabled={running}>
					{t('migration.newTask')}
				</button>
				<button
					class="btn"
					type="button"
					disabled={running}
					onclick={() => (migrationPanel.open = false)}>{t('migration.close')}</button
				>
			</div>
		</header>
		<div class="analysis-body">
			<div class="analysis-side">
				<div class="analysis-side-scroll">
					<p class="analysis-intro">{t('migration.hint')}</p>

					<section class="analysis-section">
						<h3>{t('migration.history')}</h3>
						{#if history.length === 0}
							<p class="analysis-note">{t('migration.historyEmpty')}</p>
						{:else}
							<ul class="analysis-history">
								{#each history as item (item.id)}
									<li class:active={item.id === viewingId}>
										<button
											type="button"
											class="analysis-history-open"
											disabled={running}
											onclick={() => openHistory(item.id)}
										>
											<span class="analysis-history-title">{item.title}</span>
											<span class="analysis-history-meta">
												<span class="migration-status {item.status}"
													>{migrationStatusLabel(item.status)}</span
												>
												<span>{historyTime(item.createdAt)}</span>
												<span
													>{t('migration.statements', {
														count: item.statementCount
													})}</span
												>
											</span>
										</button>
										<button
											type="button"
											class="analysis-history-delete"
											aria-label={t('migration.historyDelete')}
											disabled={running}
											onclick={() => deleteHistory(item.id)}
										>
											<X size={13} />
										</button>
									</li>
								{/each}
							</ul>
						{/if}
					</section>

					<section class="analysis-section">
						<h3>{t('migration.newTask')}</h3>
						<div class="migration-connection-actions">
							<button
								class="btn analysis-side-btn"
								type="button"
								disabled={running}
								onclick={openNewConnection}
							>
								{t('migration.newConnection')}
							</button>
						</div>
						{#if savedConnections.length === 0}
							<p class="analysis-note">{t('migration.noConnections')}</p>
						{:else}
							<form class="migration-form" onsubmit={submit}>
								<label class="field">
									<span>{t('migration.sourceConnection')}</span>
									<select
										value={sourceConnectionId}
										disabled={running || connectingSource}
										onchange={(event) =>
											void onSourceConnectionChange(
												(event.currentTarget as HTMLSelectElement).value
											)}
									>
										<option value="">{t('migration.pickConnection')}</option>
										{#each savedConnections as item (item.id)}
											<option value={item.id}>{connectionLabel(item)}</option>
										{/each}
									</select>
								</label>

								<label class="field">
									<span>{t('migration.sourceName', { noun })}</span>
									<select
										value={sourceName}
										disabled={running ||
											connectingSource ||
											loadingSchemas ||
											!sourceConnection?.connected ||
											sourceSchemas.length === 0}
										onchange={(event) =>
											void onSourceNameChange((event.currentTarget as HTMLSelectElement).value)}
									>
										<option value=""
											>{connectingSource
												? t('migration.connecting')
												: loadingSchemas
													? t('migration.loading')
													: t('migration.pickSource')}</option
										>
										{#each sourceSchemas as item (item.name)}
											<option value={item.name}>{item.name}</option>
										{/each}
									</select>
								</label>

								<label class="field">
									<span>{t('dialog.migrateTargetConnection')}</span>
									<select
										value={targetConnectionId}
										disabled={running || targetOptions.length === 0}
										onchange={(event) =>
											onTargetConnectionChange((event.currentTarget as HTMLSelectElement).value)}
									>
										{#if targetOptions.length === 0}
											<option value="">{t('dialog.migrateNoTargets')}</option>
										{:else}
											{#each targetOptions as item (item.id)}
												<option value={item.id}>
													{connectionLabel(item)}{item.id === sourceConnectionId
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
										disabled={running}
										placeholder={sourceName || nounLower}
									/>
								</label>

								<label class="field">
									<span>{t('dialog.dumpIncludeData')}</span>
									<input type="checkbox" bind:checked={includeData} disabled={running} />
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
											{#if targetCheckMessage && sourceConnectionId !== targetConnectionId}
												<span class="migration-check-detail">{targetCheckMessage}</span>
											{/if}
										</p>
									</div>
								{/if}

								<p class="hint">{t('dialog.migrateHint', { noun: nounLower })}</p>

								<button class="btn primary analysis-side-btn" type="submit" disabled={!canSubmit}>
									{running
										? checking
											? t('dialog.migrateChecking')
											: t('dialog.migrating')
										: t('dialog.migrate')}
								</button>
							</form>
						{/if}
					</section>
				</div>
			</div>

			<div class="analysis-main">
				{#if viewing}
					<p class="hint analysis-history-viewing">
						{t('migration.historyViewing', { time: historyTime(viewing.createdAt) })} ·
						{migrationStatusLabel(viewing.status)} · {viewing.title}
					</p>
				{:else if finished}
					<p class="hint">{t('migration.finished')}</p>
				{:else}
					<p class="hint">{t('migration.progressHint')}</p>
				{/if}

				{#if error}
					<p class="message error">{error}</p>
				{/if}

				{#if logs.length > 0}
					<div
						class="migrate-log migration-log-panel"
						bind:this={logBox}
						role="log"
						aria-live="polite"
					>
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
				{:else}
					<div class="analysis-empty">{t('migration.emptyLog')}</div>
				{/if}
			</div>
		</div>
	</div>
</div>
