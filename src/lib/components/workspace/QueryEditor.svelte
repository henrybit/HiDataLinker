<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { api, errorMessage } from '$lib/api/tauri';
	import type { QueryLogEntry } from '$lib/api/types';
	import { formatDuration, formatNumber } from '$lib/format';
	import { afterPaint } from '$lib/runtime/jobs';
	import { isOracle, isPostgres } from '$lib/engine';
	import { buildExplainSql } from '$lib/sql/explain';
	import { formatSql } from '$lib/sql/format';
	import { workspace } from '$lib/stores/workspace.svelte';
	import StackSplit from '$lib/components/layout/StackSplit.svelte';
	import { t } from '$lib/i18n/i18n.svelte';
	import DataGrid from './DataGrid.svelte';
	import SqlEditor from './SqlEditor.svelte';

	let {
		tabId,
		connectionId,
		schema,
		sql,
		autoRun = false,
		active = true,
		sqlCached = false
	}: {
		tabId: string;
		connectionId: string;
		schema?: string;
		sql: string;
		autoRun?: boolean;
		active?: boolean;
		sqlCached?: boolean;
	} = $props();

	let text = $state('');
	let hydrated = $state(false);
	let loadingSql = $state(false);
	let running = $state(false);
	let error = $state<string | null>(null);
	let localLog = $state<QueryLogEntry[]>([]);
	let didAutoRun = $state(false);
	const result = $derived(workspace.queryResults[tabId] ?? null);
	const connection = $derived(workspace.connections.find((item) => item.id === connectionId));
	const schemaHint = $derived(
		schema
			? isOracle(connection?.engine)
				? t('query.currentSchema', { schema })
				: isPostgres(connection?.engine)
					? t('query.searchPath', { schema })
					: t('query.useSchema', { schema })
			: t('query.noSchema')
	);
	const executionLog = $derived<QueryLogEntry[]>(
		error
			? [...localLog, { level: 'error', text: error }]
			: result?.messages?.length
				? result.messages
				: localLog
	);
	const canAct = $derived(!running && !loadingSql && Boolean(text.trim()));

	$effect(() => {
		const id = tabId;
		const cached = untrack(() => sqlCached);
		const seed = untrack(() => sql);
		let cancelled = false;
		hydrated = false;
		loadingSql = cached;
		void (async () => {
			try {
				const next = cached ? await workspace.loadTabSql(id) : seed;
				if (cancelled) return;
				text = next;
			} catch (err) {
				if (!cancelled) error = errorMessage(err);
			} finally {
				if (!cancelled) {
					loadingSql = false;
					hydrated = true;
				}
			}
		})();
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		if (!hydrated) return;
		workspace.setTabSql(tabId, text);
	});

	onDestroy(() => {
		if (hydrated) workspace.flushTabSql(tabId, text);
	});

	$effect(() => {
		void active;
		if (active) return;
		const current = untrack(() => text);
		workspace.setTabSql(tabId, current);
	});

	function format() {
		try {
			const next = formatSql(text, connection?.engine);
			if (next === text) return;
			text = next;
			workspace.setTabSql(tabId, next);
			workspace.lastMessage = null;
		} catch {
			workspace.lastMessage = t('query.formatFailed');
		}
	}

	async function execute(sqlText: string, mode: 'run' | 'explain') {
		running = true;
		error = null;
		localLog = [
			{
				level: 'info',
				text: mode === 'explain' ? t('query.log.explaining') : t('query.log.starting')
			}
		];
		workspace.setTabSql(tabId, text);
		try {
			await afterPaint();
			const queryResult = await api.executeSql(connectionId, sqlText, schema);
			localLog = [];
			const summary = queryResult.columns.length
				? t('query.summaryRows', {
						count: formatNumber(queryResult.rows.length),
						duration: formatDuration(queryResult.durationMs)
					})
				: t('query.summaryAffected', {
						count: formatNumber(queryResult.affectedRows),
						duration: formatDuration(queryResult.durationMs)
					});
			const labeled = mode === 'explain' ? t('query.summaryExplain', { summary }) : summary;
			workspace.setQueryResult(
				tabId,
				queryResult,
				queryResult.truncated ? t('query.summaryTruncated', { summary: labeled }) : labeled
			);
		} catch (err) {
			error = errorMessage(err);
			localLog = [...localLog, { level: 'info', text: t('query.log.failed') }];
			workspace.setQueryResult(tabId, null, error);
		} finally {
			running = false;
		}
	}

	async function run() {
		await execute(text, 'run');
	}

	async function explain() {
		const explainSql = buildExplainSql(text, connection?.engine);
		if (!explainSql) return;
		await execute(explainSql, 'explain');
	}

	$effect(() => {
		if (!hydrated || !autoRun || didAutoRun || running || loadingSql || !active) return;
		didAutoRun = true;
		workspace.clearTabAutoRun(tabId);
		void run();
	});
</script>

<div class="object-list">
	<div class="filter-bar">
		<button class="btn primary" onclick={run} disabled={running || loadingSql}
			>{running ? t('query.running') : t('query.run')}</button
		>
		<button class="btn" onclick={format} disabled={!canAct}>{t('query.format')}</button>
		<button class="btn" onclick={explain} disabled={!canAct}>{t('query.explain')}</button>
		<span style="color:var(--text-muted)"
			>{schemaHint} · {t('query.shortcut')} · {t('query.formatShortcut')} · {t(
				'query.explainShortcut'
			)}</span
		>
	</div>
	<StackSplit>
		{#snippet top()}
			{#if loadingSql}
				<div class="empty">{t('query.loadingSql')}</div>
			{:else}
				<SqlEditor
					bind:value={text}
					engine={connection?.engine}
					{active}
					onRun={run}
					onFormat={format}
					onExplain={explain}
				/>
			{/if}
		{/snippet}
		{#snippet bottom()}
			{#if error}
				<div class="message error">{error}</div>
			{:else if result}
				<div class="message">
					{result.statementKind}
					{#if result.columns.length}
						· {t('query.rows', { count: formatNumber(result.rows.length) })}
					{:else}
						· {t('query.affected', { count: formatNumber(result.affectedRows) })}
					{/if}
					· {formatDuration(result.durationMs)}
					{#if result.lastInsertId != null}
						· {t('query.insertId', { id: formatNumber(result.lastInsertId) })}
					{/if}
					{#if result.truncated}· {t('query.truncated')}{/if}
				</div>
			{:else if !running}
				<div class="empty">{t('query.empty')}</div>
			{/if}

			{#if executionLog.length}
				<div class="query-exec-log" aria-label={t('query.log.title')}>
					<div class="query-exec-log-title">{t('query.log.title')}</div>
					<ol class="query-exec-log-list">
						{#each executionLog as entry, index (index)}
							<li class="query-exec-log-item level-{entry.level}">
								<span class="query-exec-log-level">{entry.level}</span>
								<span class="query-exec-log-text">{entry.text}</span>
							</li>
						{/each}
					</ol>
				</div>
			{/if}

			{#if result?.columns.length}
				<DataGrid {result} />
			{/if}
		{/snippet}
	</StackSplit>
</div>
