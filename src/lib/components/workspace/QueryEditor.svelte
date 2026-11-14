<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { api, errorMessage } from '$lib/api/tauri';
	import type { QueryLogEntry } from '$lib/api/types';
	import { formatDuration, formatNumber } from '$lib/format';
	import { afterPaint } from '$lib/runtime/jobs';
	import { isOracle, isPostgres } from '$lib/engine';
	import { buildExplainSql, explainTarget } from '$lib/sql/explain';
	import { formatSql } from '$lib/sql/format';
	import { parsePgPlan, type ParsedPlan } from '$lib/sql/pg-plan';
	import { workspace } from '$lib/stores/workspace.svelte';
	import {
		gridContext,
		type AssistContext,
		type AssistObjectRef,
		type AssistOrigin,
		type AssistSeed
	} from '$lib/assist/context';
	import { AssistSession } from '$lib/assist/session.svelte';
	import { getLocale } from '$lib/i18n/i18n.svelte';
	import { selectedLlmProvider } from '$lib/llm/catalog.svelte';
	import StackSplit from '$lib/components/layout/StackSplit.svelte';
	import { t } from '$lib/i18n/i18n.svelte';
	import AssistPanel from './AssistPanel.svelte';
	import AssistResult from './AssistResult.svelte';
	import DataGrid from './DataGrid.svelte';
	import PlanView from './PlanView.svelte';
	import QueryExecLog from './QueryExecLog.svelte';
	import SqlEditor from './SqlEditor.svelte';

	let {
		tabId,
		connectionId,
		schema,
		sql,
		autoRun = false,
		active = true,
		sqlCached = false,
		assist
	}: {
		tabId: string;
		connectionId: string;
		schema?: string;
		sql: string;
		autoRun?: boolean;
		active?: boolean;
		sqlCached?: boolean;
		assist?: AssistSeed;
	} = $props();

	let text = $state('');
	let hydrated = $state(false);
	let loadingSql = $state(false);
	let running = $state(false);
	let error = $state<string | null>(null);
	let localLog = $state<QueryLogEntry[]>([]);
	let didAutoRun = $state(false);
	let analyze = $state(false);
	let plan = $state<{ parsed: ParsedPlan; raw: string } | null>(null);
	const result = $derived(workspace.queryResults[tabId] ?? null);
	const connection = $derived(workspace.connections.find((item) => item.id === connectionId));
	const postgres = $derived(isPostgres(connection?.engine));
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

	let selectionText = $state('');
	let lastRunSql = $state('');
	let assistOpen = $state(false);
	let assistOrigin = $state<AssistOrigin>('editor');
	let assistObject = $state<AssistObjectRef | null>(null);
	let assistQuestion = $state('');
	let assistShowPayload = $state(false);
	let assistPanel = $state<ReturnType<typeof AssistPanel> | undefined>();
	let sqlEditor = $state<ReturnType<typeof SqlEditor> | undefined>();
	const assistSession = new AssistSession();
	const hasModel = $derived(selectedLlmProvider() != null);
	const schemaChoices = $derived(
		(workspace.schema[connectionId]?.databases ?? []).map((item) => item.name)
	);
	const treeObject = $derived.by<AssistObjectRef | null>(() => {
		const selected = workspace.selection;
		if (
			!selected ||
			selected.connectionId !== connectionId ||
			!selected.schema ||
			selected.schema !== schema ||
			!selected.objectName ||
			(selected.objectKind !== 'table' && selected.objectKind !== 'view')
		) {
			return null;
		}
		return { schema: selected.schema, name: selected.objectName, kind: selected.objectKind };
	});
	const assistContext = $derived<AssistContext>({
		origin: assistOrigin,
		connectionId,
		connectionName: connection?.name ?? connectionId,
		engine: connection?.engine ?? '',
		connected: connection?.connected === true,
		schema: schema ?? null,
		object: assistObject ?? treeObject,
		editor: { text, selection: selectionText.trim() ? selectionText : null },
		grid: gridContext(lastRunSql || text, result, schema ?? null),
		error: null
	});

	function openAssist(origin: AssistOrigin, object: AssistObjectRef | null = null) {
		assistOrigin = origin;
		assistObject = object;
		assistOpen = true;
		assistPanel?.focus();
	}

	function closeAssist() {
		assistSession.dismiss();
		assistOpen = false;
		assistShowPayload = false;
	}

	async function submitAssist(question: string) {
		const provider = selectedLlmProvider();
		if (!provider) return;
		const outcome = await assistSession.ask({
			question,
			context: $state.snapshot(assistContext),
			provider,
			locale: getLocale(),
			oracleVersion: connection?.oracleVersion
		});
		if (outcome?.kind === 'draft' && !text.trim()) applyDraft('replace');
	}

	function applyDraft(mode: 'replace' | 'insert') {
		const outcome = assistSession.outcome;
		if (outcome?.kind !== 'draft') return;
		if (mode === 'replace' || !sqlEditor) {
			text = outcome.sql;
			workspace.setTabSql(tabId, text);
		} else {
			sqlEditor.insertText(outcome.sql);
		}
		assistSession.applied = true;
		assistQuestion = '';
	}

	$effect(() => {
		const seed = assist;
		if (!seed) return;
		untrack(() => {
			openAssist(seed.origin, seed.object ?? null);
			workspace.clearTabAssist(tabId);
		});
	});

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
		plan = null;
		localLog = [
			{
				level: 'info',
				text: mode === 'explain' ? t('query.log.explaining') : t('query.log.starting')
			}
		];
		workspace.setTabSql(tabId, text);
		if (mode === 'run') lastRunSql = sqlText;
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
		if (postgres) {
			await explainPostgres();
			return;
		}
		const explainSql = buildExplainSql(text, connection?.engine);
		if (!explainSql) return;
		await execute(explainSql, 'explain');
	}

	async function explainPostgres() {
		const target = explainTarget(text);
		if (!target) return;
		const withAnalyze = analyze;
		running = true;
		error = null;
		plan = null;
		localLog = [
			{
				level: 'info',
				text: withAnalyze ? t('query.log.explainingAnalyze') : t('query.log.explaining')
			}
		];
		workspace.setTabSql(tabId, text);
		workspace.setQueryResult(tabId, null);
		try {
			await afterPaint();
			const response = await api.explainPlan(connectionId, target, schema, withAnalyze);
			const parsed = parsePgPlan(response.plan);
			plan = { parsed, raw: response.plan };
			const summary = parsed.analyzed
				? t('plan.summaryAnalyzed', {
						count: parsed.nodes.length,
						duration: formatDuration(Math.round(parsed.executionTime ?? response.durationMs))
					})
				: t('plan.summaryEstimate', {
						count: parsed.nodes.length,
						cost: parsed.root.totalCost.toFixed(2)
					});
			localLog = [
				...localLog,
				{ level: 'success', text: summary },
				...(parsed.analyzed ? [{ level: 'notice' as const, text: t('plan.rolledBack') }] : [])
			];
			workspace.lastMessage = t('query.summaryExplain', { summary });
		} catch (err) {
			error = errorMessage(err);
			localLog = [...localLog, { level: 'info', text: t('query.log.failed') }];
		} finally {
			running = false;
		}
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
		<button
			class="btn"
			onclick={() => openAssist('editor')}
			disabled={loadingSql}
			title={t('assist.shortcut')}>{t('assist.open')}</button
		>
		{#if postgres}
			<label class="explain-analyze" title={t('query.explainAnalyzeHint')}>
				<input type="checkbox" bind:checked={analyze} disabled={running} />
				{t('query.explainAnalyze')}
			</label>
		{/if}
		<span style="color:var(--text-muted)"
			>{schemaHint} · {t('query.shortcut')} · {t('query.formatShortcut')} · {t(
				'query.explainShortcut'
			)}</span
		>
	</div>
	{#if assistOpen}
		<AssistPanel
			bind:this={assistPanel}
			bind:question={assistQuestion}
			bind:showPayload={assistShowPayload}
			context={assistContext}
			{hasModel}
			schemas={schemaChoices}
			busy={assistSession.busy}
			onSubmit={(question) => void submitAssist(question)}
			onCancel={() => assistSession.cancel()}
			onClose={closeAssist}
			onPickSchema={(next) => next && workspace.setTabSchema(tabId, next)}
		>
			<AssistResult
				session={assistSession}
				editorHasText={Boolean(text.trim())}
				hasSelection={Boolean(selectionText.trim())}
				onApply={applyDraft}
				onRetry={() => void submitAssist(assistSession.question)}
			/>
		</AssistPanel>
	{/if}
	<StackSplit>
		{#snippet top()}
			{#if loadingSql}
				<div class="empty">{t('query.loadingSql')}</div>
			{:else}
				<SqlEditor
					bind:this={sqlEditor}
					bind:value={text}
					bind:selection={selectionText}
					engine={connection?.engine}
					{active}
					onRun={run}
					onFormat={format}
					onExplain={explain}
					onAssist={() => openAssist('editor')}
				/>
			{/if}
		{/snippet}
		{#snippet bottom()}
			{#if error}
				<div class="message error">{error}</div>
			{:else if plan}
				<div class="message">
					{plan.parsed.analyzed ? 'EXPLAIN ANALYZE' : 'EXPLAIN'} · {t('plan.nodes', {
						count: plan.parsed.nodes.length
					})}
				</div>
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
					{#if result.columns.length}
						<button class="assist-inline" type="button" onclick={() => openAssist('grid')}
							>{t('assist.askResult')}</button
						>
					{/if}
				</div>
			{/if}

			<div class="query-results-area">
				{#if plan && !error}
					<PlanView plan={plan.parsed} raw={plan.raw} />
				{:else if result?.columns.length}
					<DataGrid {result} />
				{:else if !error && !result && !running}
					<div class="empty">{t('query.empty')}</div>
				{/if}
			</div>

			{#if executionLog.length}
				<QueryExecLog entries={executionLog} />
			{/if}
		{/snippet}
	</StackSplit>
</div>

<style>
	.assist-inline {
		margin-left: 8px;
		padding: 0;
		border: none;
		background: none;
		color: var(--accent);
		cursor: pointer;
	}
</style>
