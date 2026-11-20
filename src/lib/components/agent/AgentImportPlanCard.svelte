<script lang="ts">
	import { FileSpreadsheet, LoaderCircle, Sparkles } from '@lucide/svelte';
	import { z } from 'zod';
	import { api, errorMessage } from '$lib/api/tauri';
	import type {
		AttachmentMeta,
		DatabaseInfo,
		ParseSample,
		SuggestedColumn,
		TableInfo
	} from '$lib/api/types';
	import { createAnalysisCaller } from '$lib/analysis/llm';
	import { getLocale, t } from '$lib/i18n/i18n.svelte';
	import {
		columnLengthHints,
		importSqlRepairPrompt,
		repairImportSqlLocally,
		schemaSqlFromPreview,
		stripSqlFence
	} from '$lib/import/repair-sql';
	import { selectedLlmProvider } from '$lib/llm/catalog.svelte';
	import { toLlmSettings } from '$lib/llm/providers';
	import { agentChat, DATA_ANALYST_ID, type ChatMessage } from '$lib/stores/agent-chat.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';

	let {
		message
	}: {
		message: ChatMessage;
	} = $props();

	const attachment = $derived((message.meta?.attachment ?? null) as AttachmentMeta | null);
	const sample = $derived((message.meta?.sample ?? null) as ParseSample | null);
	const importing = $derived(message.meta?.importing === true);
	const awaitingDecision = $derived(message.meta?.awaitingDecision === true);
	const phase = $derived(String(message.meta?.phase ?? 'plan'));
	const progressMessage = $derived(String(message.meta?.progressMessage ?? ''));
	const writtenRows = $derived(Number(message.meta?.writtenRows ?? 0));
	const totalRows = $derived(Number(message.meta?.totalRows ?? sample?.totalRows ?? 0));
	const scriptPreview = $derived(String(message.meta?.scriptPreview ?? ''));
	const jobId = $derived(String(message.meta?.jobId ?? ''));
	const decided = $derived(message.meta?.decision != null);

	let mode = $state<'create' | 'append'>('create');
	let connectionId = $state('');
	let schema = $state('');
	let table = $state('');
	let ddl = $state('');
	let ddlTouched = $state(false);
	let columns = $state<SuggestedColumn[]>([]);
	let schemas = $state<DatabaseInfo[]>([]);
	let tables = $state<TableInfo[]>([]);
	let busy = $state(false);
	let fixing = $state(false);
	let fixNote = $state<string | null>(null);
	let localError = $state<string | null>(null);
	let showScript = $state(false);

	const formLocked = $derived(
		busy || importing || phase === 'done' || (decided && phase !== 'error')
	);
	const sqlLocked = $derived(busy || importing || phase === 'done' || fixing);
	const importError = $derived(localError || String(message.meta?.error ?? '') || '');
	const loadNote = $derived.by(() => {
		const preview = scriptPreview;
		const marker = preview.search(/\n-- Import load plan\b/);
		return marker === -1 ? '' : preview.slice(marker).trim();
	});

	const connections = $derived(
		workspace.connections.filter((item) => {
			const engine = item.engine.toLowerCase();
			return engine.includes('mysql') || engine.includes('postgres');
		})
	);

	const selectedConnection = $derived(
		connections.find((item) => item.id === connectionId) ?? null
	);

	$effect(() => {
		if (!connectionId && connections.length > 0) {
			connectionId = connections[0]?.id ?? '';
		}
	});

	$effect(() => {
		if (sample && !table) {
			table = sample.suggestedTable;
		}
	});

	$effect(() => {
		if (ddlTouched) return;
		const saved = String(message.meta?.editedDdl ?? '').trim();
		const fromPreview = schemaSqlFromPreview(String(message.meta?.scriptPreview ?? ''));
		const next = saved || fromPreview;
		if (!next) return;
		ddl = next;
		ddlTouched = true;
	});

	$effect(() => {
		if (!connectionId || !selectedConnection || !sample) return;
		let cancelled = false;
		void (async () => {
			try {
				await api.connect(connectionId);
				if (cancelled) return;
				schemas = await api.listDatabases(connectionId);
				if (cancelled) return;
				if (!schema && schemas[0]) schema = schemas[0].name;
				const inferred = await api.importInferSchema(
					{ sample, engine: selectedConnection.engine },
					schema,
					table || sample.suggestedTable
				);
				if (cancelled) return;
				columns = inferred.columns;
				if (!ddlTouched) ddl = inferred.ddl;
				if (!table) table = inferred.table;
			} catch (error) {
				if (!cancelled) localError = errorMessage(error);
			}
		})();
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		if (!connectionId || !schema || mode !== 'append') {
			tables = [];
			return;
		}
		let cancelled = false;
		void (async () => {
			try {
				await api.connect(connectionId);
				if (cancelled) return;
				tables = await api.listTables(connectionId, schema);
				if (cancelled) return;
				if (!table && tables[0]) table = tables[0].name;
			} catch (error) {
				if (!cancelled) localError = errorMessage(error);
			}
		})();
		return () => {
			cancelled = true;
		};
	});

	function onSqlInput() {
		ddlTouched = true;
		fixNote = null;
		agentChat.updateImportMessage(message.id, { editedDdl: ddl });
	}

	function applyRepairedSql(sql: string, note: string) {
		ddl = sql.trim();
		ddlTouched = true;
		fixNote = note;
		showScript = true;
		agentChat.updateImportMessage(message.id, { editedDdl: ddl });
	}

	async function fixImportError() {
		const errorText = importError.trim();
		const currentSql = (ddl || schemaSqlFromPreview(scriptPreview)).trim();
		if (!errorText || !currentSql) return;
		fixing = true;
		localError = null;
		try {
			const locale = getLocale() === 'zh' ? 'zh' : 'en';
			const prompt = importSqlRepairPrompt({
				engine: selectedConnection?.engine ?? '',
				error: errorText,
				sql: currentSql,
				columnHints: columnLengthHints(sample),
				locale
			});
			const profile = selectedLlmProvider();
			if (!profile || (!profile.apiKey.trim() && !profile.baseUrl.trim())) {
				throw new Error(t('agentMode.live.needLlm'));
			}
			const caller = createAnalysisCaller(toLlmSettings(profile));
			const repaired = await caller.complete(
				z.object({
					sql: z.string().min(1),
					summary: z.string()
				}),
				prompt.system,
				prompt.human
			);
			const sql = stripSqlFence(repaired.sql);
			if (!/^\s*(DROP|CREATE|ALTER)\b/i.test(sql)) {
				throw new Error(t('agentMode.live.importAiFixInvalid'));
			}
			applyRepairedSql(sql, repaired.summary.trim() || t('agentMode.live.importAiFixApplied'));
		} catch (error) {
			const local = repairImportSqlLocally(currentSql, errorText);
			if (local) {
				applyRepairedSql(local.sql, t('agentMode.live.importAiFixLocal'));
			} else {
				localError = errorMessage(error);
			}
		} finally {
			fixing = false;
		}
	}

	function decisionTarget(imported: boolean, jobId?: string) {
		return {
			messageId: message.id,
			connectionId: connectionId.trim(),
			schema: schema.trim(),
			table: table.trim(),
			mode,
			imported,
			...(jobId ? { jobId } : {})
		};
	}

	async function continueAnalyze() {
		if (!sample || !selectedConnection) return;
		if (!schema.trim() || !table.trim()) {
			localError = t('agentMode.live.importNeedTarget');
			return;
		}
		busy = true;
		localError = null;
		try {
			await agentChat.resumeAfterImportDecision(decisionTarget(false));
		} catch (error) {
			localError = errorMessage(error);
		} finally {
			busy = false;
		}
	}

	async function confirmImport() {
		if (!attachment || !sample || !selectedConnection || !agentChat.session) return;
		if (!schema.trim() || !table.trim()) {
			localError = t('agentMode.live.importNeedTarget');
			return;
		}
		const editedSql = ddl.trim();
		if (mode === 'create' && !editedSql) {
			localError = t('agentMode.live.importNeedSql');
			return;
		}
		busy = true;
		localError = null;
		fixNote = null;
		try {
			const pkg = await api.importPrepare({
				agentId: DATA_ANALYST_ID,
				sessionId: agentChat.session.id,
				attachmentPath: attachment.path,
				attachmentName: attachment.name,
				connectionId: selectedConnection.id,
				engine: selectedConnection.engine,
				schema: schema.trim(),
				table: table.trim(),
				mode,
				createTable: mode === 'create',
				ddl: mode === 'create' ? editedSql : undefined,
				columnMappings: columns.map((col) => ({
					source: col.sourceName,
					target: col.name
				}))
			});
			let preview = '';
			try {
				const schemaSql = await api.importReadTextFile(pkg.schemaSqlPath);
				const loadSql = await api.importReadTextFile(pkg.loadSqlPath);
				preview = `${schemaSql.trim()}\n\n${loadSql.trim()}`;
			} catch {
				preview = pkg.ddl;
			}
			agentChat.updateImportMessage(message.id, {
				jobId: pkg.jobId,
				scriptDir: pkg.scriptDir,
				scriptPreview: preview,
				editedDdl: mode === 'create' ? editedSql : undefined,
				phase: 'ready',
				totalRows: pkg.totalRows,
				error: null
			});
			showScript = true;

			// Non-blocking: start returns immediately; progress via events.
			await api.importStart({ jobId: pkg.jobId, scriptDir: pkg.scriptDir });
			agentChat.updateImportMessage(message.id, {
				importing: true,
				phase: 'load',
				progressMessage: t('agentMode.live.importStarting'),
				error: null
			});

			// Wait for native import-done/error before the agent continues.
			if (agentChat.awaitingImportDecision) {
				await agentChat.resumeAfterImportDecision(decisionTarget(true, pkg.jobId));
			}
		} catch (error) {
			localError = errorMessage(error);
		} finally {
			busy = false;
		}
	}

	async function cancelImport() {
		if (!jobId) return;
		try {
			await api.importCancel(jobId);
		} catch (error) {
			localError = errorMessage(error);
		}
	}
</script>

<div class="agent-import-card">
	<div class="agent-import-head">
		<span class="agent-import-icon"><FileSpreadsheet size={16} /></span>
		<div>
			<p class="agent-import-title">{message.content}</p>
			{#if sample}
				<p class="agent-import-sub">
					{t('agentMode.live.importSampleMeta', {
						rows: sample.totalRows,
						cols: sample.columnCount
					})}
				</p>
			{/if}
		</div>
	</div>

	{#if sample}
		<div class="agent-import-sample">
			<table>
				<thead>
					<tr>
						{#each sample.headers as header}
							<th>{header}</th>
						{/each}
					</tr>
				</thead>
				<tbody>
					{#each sample.sampleRows.slice(0, 5) as row}
						<tr>
							{#each sample.headers as _, index}
								<td>{row[index] ?? ''}</td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}

	{#if phase !== 'done'}
		<div class="agent-import-form">
			<label>
				<span>{t('agentMode.live.importMode')}</span>
				<select bind:value={mode} disabled={formLocked}>
					<option value="create">{t('agentMode.live.importModeCreate')}</option>
					<option value="append">{t('agentMode.live.importModeAppend')}</option>
				</select>
			</label>
			<label>
				<span>{t('agentMode.live.importConnection')}</span>
				<select
					bind:value={connectionId}
					disabled={formLocked || connections.length === 0}
				>
					{#if connections.length === 0}
						<option value="">{t('agentMode.live.importNoMysqlPg')}</option>
					{:else}
						{#each connections as conn}
							<option value={conn.id}>{conn.name} ({conn.engine})</option>
						{/each}
					{/if}
				</select>
			</label>
			<label>
				<span>{t('agentMode.live.importSchema')}</span>
				<select bind:value={schema} disabled={formLocked || schemas.length === 0}>
					{#each schemas as item}
						<option value={item.name}>{item.name}</option>
					{/each}
				</select>
			</label>
			<label>
				<span>{t('agentMode.live.importTable')}</span>
				{#if mode === 'append'}
					<select bind:value={table} disabled={formLocked}>
						{#each tables as item}
							<option value={item.name}>{item.name}</option>
						{/each}
					</select>
				{:else}
					<input bind:value={table} disabled={formLocked} />
				{/if}
			</label>
		</div>

		{#if mode === 'create' && (ddl || showScript)}
			<details class="agent-import-ddl" open>
				<summary>{t('agentMode.live.importViewScript')}</summary>
				<p class="agent-import-sql-hint">{t('agentMode.live.importSqlEditable')}</p>
				<textarea
					class="agent-import-sql"
					rows="12"
					spellcheck="false"
					bind:value={ddl}
					disabled={sqlLocked}
					oninput={onSqlInput}
				></textarea>
				{#if loadNote}
					<pre>{loadNote}</pre>
				{/if}
			</details>
		{/if}
	{/if}

	{#if importing || phase === 'load' || phase === 'ddl' || phase === 'prepare'}
		<div class="agent-import-progress">
			<span class="agent-import-spin"><LoaderCircle size={14} /></span>
			<span
				>{progressMessage ||
					t('agentMode.live.importProgress', {
						written: writtenRows,
						total: totalRows
					})}</span
			>
			<button type="button" class="agent-btn ghost" onclick={() => void cancelImport()}>
				{t('agentMode.live.cancel')}
			</button>
		</div>
	{:else if phase === 'done'}
		<p class="agent-import-done">{progressMessage}</p>
	{:else}
		<div class="agent-import-actions">
			{#if awaitingDecision}
				<button
					type="button"
					class="agent-btn ghost"
					disabled={busy || !connectionId || !schema || !table}
					onclick={() => void continueAnalyze()}
				>
					{t('agentMode.live.importAnalyzeOnly')}
				</button>
			{/if}
			<button
				type="button"
				class="agent-btn primary"
				disabled={busy || !connectionId || !schema || !table}
				onclick={() => void confirmImport()}
			>
				{busy
					? t('agentMode.live.importPreparing')
					: phase === 'error'
						? t('agentMode.live.importRetry')
						: t('agentMode.live.importConfirm')}
			</button>
		</div>
		{#if awaitingDecision}
			<p class="agent-import-wait-hint">{t('agentMode.live.importWaitHint')}</p>
		{/if}
	{/if}

	{#if importError}
		<div class="agent-import-error">
			<p class="agent-error-banner inline">{importError}</p>
			{#if phase !== 'done' && (ddl || scriptPreview)}
				<button
					type="button"
					class="agent-btn ghost agent-import-fix"
					disabled={fixing || busy || importing}
					onclick={() => void fixImportError()}
				>
					{#if fixing}
						<span class="agent-import-spin"><LoaderCircle size={14} /></span>
						{t('agentMode.live.importAiFixing')}
					{:else}
						<Sparkles size={14} />
						{t('agentMode.live.importAiFix')}
					{/if}
				</button>
			{/if}
		</div>
	{/if}
	{#if fixNote}
		<p class="agent-import-fix-note">{fixNote}</p>
	{/if}
</div>
