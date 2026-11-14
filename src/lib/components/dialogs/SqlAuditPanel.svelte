<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
	import type {
		ConnectionListItem,
		SqlAuditDay,
		SqlAuditEntry,
		SqlAuditSource
	} from '$lib/api/types';
	import Switch from '$lib/components/layout/Switch.svelte';
	import { t, type MessageKey } from '$lib/i18n/i18n.svelte';

	const PAGE_SIZE = 200;
	const SOURCES: SqlAuditSource[] = [
		'editor',
		'table_browse',
		'table_edit',
		'structure',
		'analysis',
		'schema',
		'export',
		'migration',
		'import',
		'agent',
		'assist',
		'app'
	];

	let enabled = $state(true);
	let retainDays = $state(90);
	let dir = $state('~/.hidatalinker/audit');
	let settingsBusy = $state(false);
	let settingsError = $state<string | null>(null);

	let days = $state<SqlAuditDay[]>([]);
	let connections = $state<ConnectionListItem[]>([]);
	let date = $state(today());
	let connectionId = $state('');
	let source = $state('');
	let status = $state<'' | 'ok' | 'error'>('');
	let keyword = $state('');

	let entries = $state<SqlAuditEntry[]>([]);
	let total = $state(0);
	let loading = $state(false);
	let loadError = $state<string | null>(null);
	let expandedId = $state<string | null>(null);
	let copiedId = $state<string | null>(null);

	function today(): string {
		const now = new Date();
		const pad = (value: number) => String(value).padStart(2, '0');
		return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
	}

	function sourceLabel(value: string): string {
		return t(`settings.audit.source.${value}` as MessageKey);
	}

	/** Source plus whether an assistant draft was only drafted, only written, or actually run. */
	function sourceText(entry: SqlAuditEntry): string {
		const label = sourceLabel(entry.source);
		if (entry.operation === 'draft' || entry.operation === 'draft_applied') {
			return `${label} · ${t(`settings.audit.op.${entry.operation}`)}`;
		}
		if (entry.sourceDetail?.startsWith('assist:'))
			return `${label} · ${t('settings.audit.ranDraft')}`;
		return label;
	}

	function timeOf(at: string): string {
		const match = /T(\d{2}:\d{2}:\d{2}(?:\.\d+)?)/.exec(at);
		return match ? match[1] : at;
	}

	function oneLine(sql: string): string {
		return sql.replace(/\s+/g, ' ').trim();
	}

	async function loadSettings() {
		try {
			const info = await api.readSqlAuditSettings();
			enabled = info.settings.enabled;
			retainDays = info.settings.retainDays;
			dir = info.dir;
		} catch (caught) {
			console.error('[sql-audit]', caught);
		}
	}

	async function saveSettings(next: { enabled: boolean; retainDays: number }) {
		settingsBusy = true;
		settingsError = null;
		try {
			const info = await api.writeSqlAuditSettings(next);
			enabled = info.settings.enabled;
			retainDays = info.settings.retainDays;
			await loadDays();
		} catch (caught) {
			console.error('[sql-audit]', caught);
			settingsError = t('settings.audit.saveFailed');
			await loadSettings();
		} finally {
			settingsBusy = false;
		}
	}

	async function loadDays() {
		try {
			days = await api.listSqlAuditDays();
		} catch (caught) {
			console.error('[sql-audit]', caught);
		}
	}

	async function load(append = false) {
		loading = true;
		loadError = null;
		try {
			const page = await api.listSqlAudit({
				date,
				connectionId: connectionId || null,
				source: source || null,
				status: status || null,
				keyword: keyword.trim() || null,
				limit: PAGE_SIZE,
				offset: append ? entries.length : 0
			});
			entries = append ? [...entries, ...page.entries] : page.entries;
			total = page.total;
		} catch (caught) {
			loadError = `${t('settings.audit.loadFailed')} ${errorMessage(caught)}`;
		} finally {
			loading = false;
		}
	}

	async function refresh() {
		expandedId = null;
		await Promise.all([loadDays(), load()]);
	}

	async function copySql(entry: SqlAuditEntry) {
		try {
			await navigator.clipboard.writeText(entry.sql);
			copiedId = entry.id;
			setTimeout(() => {
				if (copiedId === entry.id) copiedId = null;
			}, 1500);
		} catch (caught) {
			console.error('[sql-audit]', caught);
		}
	}

	const dateOptions = $derived(
		days.some((day) => day.date === date) ? days : [{ date, sizeBytes: 0 }, ...days]
	);

	onMount(() => {
		if (!isTauriRuntime()) return;
		void loadSettings();
		void refresh();
		api
			.listConnections()
			.then((items) => (connections = items))
			.catch((caught) => console.error('[sql-audit]', caught));
	});
</script>

{#if !isTauriRuntime()}
	<p class="hint">{t('settings.audit.desktopOnly')}</p>
{:else}
	<section class="mcp-settings">
		<div class="mcp-settings-copy">
			<strong>{t('settings.audit.title')}</strong>
			<p class="hint">{t('settings.audit.hint', { dir })}</p>
			{#if settingsError}<p class="settings-error">{settingsError}</p>{/if}
		</div>
		<div class="audit-settings-controls">
			<label class="audit-retain">
				<span>{t('settings.audit.retain')}</span>
				<input
					type="number"
					min="1"
					max="3650"
					value={retainDays}
					disabled={settingsBusy}
					onchange={(event) => {
						const value = Number((event.currentTarget as HTMLInputElement).value);
						if (Number.isFinite(value) && value > 0) {
							void saveSettings({ enabled, retainDays: Math.round(value) });
						}
					}}
				/>
			</label>
			<Switch
				name={t('settings.audit.title')}
				checked={enabled}
				disabled={settingsBusy}
				onText={t('settings.audit.on')}
				offText={t('settings.audit.off')}
				onToggle={(next) => saveSettings({ enabled: next, retainDays })}
			/>
		</div>
	</section>

	<div class="audit-filters">
		<select
			class="audit-control"
			aria-label={t('settings.audit.date')}
			bind:value={date}
			onchange={() => load()}
		>
			{#each dateOptions as day (day.date)}
				<option value={day.date}>{day.date}</option>
			{/each}
		</select>
		<select
			class="audit-control"
			aria-label={t('settings.audit.connection')}
			bind:value={connectionId}
			onchange={() => load()}
		>
			<option value="">{t('settings.audit.allConnections')}</option>
			{#each connections as connection (connection.id)}
				<option value={connection.id}>{connection.name}</option>
			{/each}
		</select>
		<select
			class="audit-control"
			aria-label={t('settings.audit.source')}
			bind:value={source}
			onchange={() => load()}
		>
			<option value="">{t('settings.audit.allSources')}</option>
			{#each SOURCES as item (item)}
				<option value={item}>{sourceLabel(item)}</option>
			{/each}
		</select>
		<select
			class="audit-control"
			aria-label={t('settings.audit.status')}
			bind:value={status}
			onchange={() => load()}
		>
			<option value="">{t('settings.audit.allStatus')}</option>
			<option value="ok">{t('settings.audit.ok')}</option>
			<option value="error">{t('settings.audit.failed')}</option>
		</select>
		<input
			class="audit-control audit-keyword"
			type="search"
			placeholder={t('settings.audit.keyword')}
			bind:value={keyword}
			onkeydown={(event) => {
				if (event.key === 'Enter') {
					event.preventDefault();
					void load();
				}
			}}
		/>
		<button class="btn" type="button" disabled={loading} onclick={refresh}>
			{t('settings.audit.refresh')}
		</button>
	</div>

	{#if loadError}<p class="settings-error">{loadError}</p>{/if}

	<div class="audit-table">
		<div class="audit-grid audit-head">
			<span>{t('settings.audit.col.time')}</span>
			<span>{t('settings.audit.status')}</span>
			<span>{t('settings.audit.source')}</span>
			<span>{t('settings.audit.connection')}</span>
			<span>SQL</span>
			<span class="audit-num">{t('settings.audit.col.duration')}</span>
		</div>
		<div class="audit-list">
			{#each entries as entry (entry.id)}
				<div class="audit-row" class:failed={!entry.success}>
					<button
						class="audit-grid audit-summary"
						type="button"
						aria-expanded={expandedId === entry.id}
						onclick={() => (expandedId = expandedId === entry.id ? null : entry.id)}
					>
						<span class="audit-mono">{timeOf(entry.at)}</span>
						<span class="audit-status" class:ok={entry.success}>
							{entry.success ? t('settings.audit.ok') : t('settings.audit.failed')}
						</span>
						<span>{sourceText(entry)}</span>
						<span
							title={`${entry.connectionName}${entry.schema ? ` / ${entry.schema}` : ''} (${entry.host}:${entry.port})`}
						>
							{entry.connectionName}{entry.schema ? ` / ${entry.schema}` : ''}
						</span>
						<code class="audit-mono">{oneLine(entry.sql)}</code>
						<span class="audit-mono audit-num">{entry.durationMs} ms</span>
					</button>
					{#if expandedId === entry.id}
						<div class="audit-detail">
							<dl>
								<dt>{t('settings.audit.connection')}</dt>
								<dd>
									{entry.connectionName} ({entry.engine}) {entry.username}@{entry.host}:{entry.port}
								</dd>
								<dt>{t('settings.audit.source')}</dt>
								<dd>
									{sourceLabel(entry.source)}{entry.sourceDetail ? ` · ${entry.sourceDetail}` : ''}
								</dd>
								<dt>{t('settings.audit.operation')}</dt>
								<dd>
									{entry.operation}{entry.statementKind ? ` · ${entry.statementKind}` : ''}
									{#if entry.rowCount != null}
										· {t('settings.audit.rows', { count: entry.rowCount })}{/if}
									{#if entry.affectedRows != null}
										· {t('settings.audit.affected', { count: entry.affectedRows })}{/if}
								</dd>
								<dt>{t('settings.audit.user')}</dt>
								<dd>{entry.osUser || '-'} · {entry.at}</dd>
							</dl>
							{#if entry.error}<div class="audit-error">{entry.error}</div>{/if}
							<pre class="audit-sql-full">{entry.sql}</pre>
							{#if entry.sqlTruncated}
								<p class="hint">{t('settings.audit.truncated', { length: entry.sqlLength })}</p>
							{/if}
							<button class="btn" type="button" onclick={() => copySql(entry)}>
								{copiedId === entry.id ? t('settings.audit.copied') : t('settings.audit.copy')}
							</button>
						</div>
					{/if}
				</div>
			{:else}
				{#if !loading}<p class="hint audit-empty">{t('settings.audit.empty')}</p>{/if}
			{/each}
		</div>
	</div>
	<div class="audit-footer">
		<span class="hint">{t('settings.audit.total', { count: total })}</span>
		{#if entries.length < total}
			<button class="btn" type="button" disabled={loading} onclick={() => load(true)}>
				{t('settings.audit.loadMore')}
			</button>
		{/if}
	</div>
{/if}

<style>
	.audit-settings-controls {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-shrink: 0;
	}

	.audit-retain {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--text-muted);
		font-size: 12px;
		white-space: nowrap;
	}

	.audit-retain input,
	.audit-control {
		height: 28px;
		padding: 0 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: white;
		color: var(--text);
		font-size: 12px;
		outline: none;
	}

	.audit-retain input {
		width: 72px;
	}

	.audit-retain input:focus,
	.audit-control:focus {
		border-color: #60a5fa;
		box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.15);
	}

	.audit-filters {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.audit-filters select {
		flex: 0 1 150px;
		min-width: 0;
	}

	.audit-keyword {
		flex: 1 1 200px;
		min-width: 120px;
	}

	.audit-filters .btn {
		flex-shrink: 0;
	}

	.audit-table {
		overflow: hidden;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg-panel);
	}

	.audit-grid {
		display: grid;
		grid-template-columns: 96px 72px 96px minmax(100px, 180px) minmax(0, 1fr) 72px;
		align-items: center;
		gap: 10px;
		padding: 0 10px;
	}

	.audit-grid > * {
		min-width: 0;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
	}

	.audit-head {
		height: 30px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-muted);
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
	}

	.audit-list {
		max-height: 340px;
		overflow: auto;
		user-select: text;
	}

	.audit-row + .audit-row {
		border-top: 1px solid var(--border);
	}

	.audit-summary {
		width: 100%;
		height: 32px;
		border: 0;
		background: transparent;
		color: var(--text);
		font-size: 12px;
		text-align: left;
	}

	.audit-summary:hover {
		background: var(--bg-hover);
	}

	.audit-row.failed .audit-summary {
		background: #fef2f2;
	}

	.audit-mono {
		font-family: var(--mono);
		font-size: 11px;
	}

	.audit-num {
		text-align: right;
	}

	.audit-status {
		color: var(--danger);
		font-weight: 600;
	}

	.audit-status.ok {
		color: var(--success);
	}

	.audit-detail {
		display: grid;
		gap: 8px;
		padding: 10px 12px 12px;
		border-top: 1px dashed var(--border);
		background: var(--bg-muted);
	}

	.audit-detail dl {
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: 4px 12px;
		margin: 0;
		font-size: 12px;
	}

	.audit-detail dt {
		color: var(--text-muted);
	}

	.audit-detail dd {
		margin: 0;
		word-break: break-all;
	}

	.audit-sql-full {
		max-height: 220px;
		margin: 0;
		padding: 8px;
		overflow: auto;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--bg-panel);
		font-family: var(--mono);
		font-size: 12px;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.audit-error {
		color: var(--danger);
		font-size: 12px;
		word-break: break-word;
	}

	.audit-detail .btn {
		justify-self: start;
	}

	.audit-empty {
		margin: 0;
		padding: 24px 16px;
		text-align: center;
	}

	.audit-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		min-height: 28px;
	}
</style>
