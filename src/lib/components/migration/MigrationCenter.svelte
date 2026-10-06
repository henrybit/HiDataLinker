<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { listen, type UnlistenFn } from '@tauri-apps/api/event';
	import { X } from '@lucide/svelte';
	import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
	import type { MigrateProgressEvent } from '$lib/api/types';
	import { normalizeEngine } from '$lib/engine';
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

	const sourceConnection = $derived(
		workspace.connections.find((item) => item.id === sourceConnectionId) ?? null
	);
	const connectedSources = $derived(workspace.connections.filter((item) => item.connected));
	const sourceSchemas = $derived(
		sourceConnectionId ? (workspace.schema[sourceConnectionId]?.databases ?? []) : []
	);
	const targetOptions = $derived.by(() => {
		if (!sourceConnection) return [];
		const engine = normalizeEngine(sourceConnection.engine);
		return workspace.connections.filter(
			(item) => item.connected && normalizeEngine(item.engine) === engine
		);
	});
	const noun = $derived(schemaNounLabel(sourceConnection?.engine));
	const nounLower = $derived(schemaNounLower(sourceConnection?.engine));
	const canSubmit = $derived(
		!!sourceConnectionId &&
			!!sourceName.trim() &&
			!!targetConnectionId &&
			!!targetName.trim() &&
			!running &&
			!(sourceConnectionId === targetConnectionId && sourceName.trim() === targetName.trim())
	);

	onMount(() => {
		void loadHistory();
		const first = connectedSources[0];
		if (first) {
			sourceConnectionId = first.id;
			targetConnectionId = first.id;
			void ensureSourceSchemas(first.id);
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
			migrationPanel.open = false;
		};
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
	});

	onDestroy(() => {
		unlisten?.();
		unlisten = null;
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

	async function onSourceConnectionChange(connectionId: string) {
		sourceConnectionId = connectionId;
		sourceName = '';
		targetName = '';
		const options = workspace.connections.filter(
			(item) =>
				item.connected &&
				normalizeEngine(item.engine) ===
					normalizeEngine(workspace.connections.find((c) => c.id === connectionId)?.engine)
		);
		if (!options.some((item) => item.id === targetConnectionId)) {
			targetConnectionId = options[0]?.id ?? '';
		}
		await ensureSourceSchemas(connectionId);
	}

	async function onSourceNameChange(name: string) {
		sourceName = name;
		if (!targetName.trim() || targetName === sourceName) {
			targetName = name;
		}
	}

	function startNewTask() {
		viewingId = null;
		logs = [];
		finished = false;
		error = null;
	}

	async function submit(event?: SubmitEvent) {
		event?.preventDefault();
		if (!canSubmit || !sourceConnection) return;
		if (!isTauriRuntime()) {
			error = t('migration.needTauri');
			return;
		}
		const target = workspace.connections.find((item) => item.id === targetConnectionId);
		if (!target?.connected) {
			error = t('migration.needTarget');
			return;
		}
		viewingId = null;
		running = true;
		finished = false;
		error = null;
		logs = [];
		const sourceLabel = sourceConnection.name;
		const targetLabel = target.name;
		const source = sourceName.trim();
		const destination = targetName.trim();
		let statementCount = 0;
		let failed: string | null = null;
		try {
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
						{#if connectedSources.length === 0}
							<p class="analysis-note">{t('migration.needConnected')}</p>
						{:else}
							<form class="migration-form" onsubmit={submit}>
								<label class="field">
									<span>{t('migration.sourceConnection')}</span>
									<select
										value={sourceConnectionId}
										disabled={running}
										onchange={(event) =>
											void onSourceConnectionChange(
												(event.currentTarget as HTMLSelectElement).value
											)}
									>
										{#each connectedSources as item (item.id)}
											<option value={item.id}>{item.name}</option>
										{/each}
									</select>
								</label>

								<label class="field">
									<span>{t('migration.sourceName', { noun })}</span>
									<select
										value={sourceName}
										disabled={running || loadingSchemas || sourceSchemas.length === 0}
										onchange={(event) =>
											void onSourceNameChange((event.currentTarget as HTMLSelectElement).value)}
									>
										<option value=""
											>{loadingSchemas ? t('migration.loading') : t('migration.pickSource')}</option
										>
										{#each sourceSchemas as item (item.name)}
											<option value={item.name}>{item.name}</option>
										{/each}
									</select>
								</label>

								<label class="field">
									<span>{t('dialog.migrateTargetConnection')}</span>
									<select
										bind:value={targetConnectionId}
										disabled={running || targetOptions.length === 0}
									>
										{#if targetOptions.length === 0}
											<option value="">{t('dialog.migrateNoTargets')}</option>
										{:else}
											{#each targetOptions as item (item.id)}
												<option value={item.id}>
													{item.name}
													{item.id === sourceConnectionId ? t('dialog.migrateSameConnection') : ''}
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

								<p class="hint">{t('dialog.migrateHint', { noun: nounLower })}</p>

								<button class="btn primary analysis-side-btn" type="submit" disabled={!canSubmit}>
									{running ? t('dialog.migrating') : t('dialog.migrate')}
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
