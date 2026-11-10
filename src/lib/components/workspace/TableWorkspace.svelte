<script lang="ts">
	import { api, errorMessage } from '$lib/api/tauri';
	import { formatDuration, formatNumber } from '$lib/format';
	import { afterPaint } from '$lib/runtime/jobs';
	import { workspace } from '$lib/stores/workspace.svelte';
	import type { ColumnInfo, QueryResult } from '$lib/api/types';
	import { toInputValue, temporalKind } from '$lib/sql/column-types';
	import {
		buildDeleteSql,
		buildInsertSql,
		buildInsertSqlFromRow,
		buildUpdateSql,
		editableColumns,
		insertableColumns,
		primaryKeyColumns
	} from '$lib/sql/row-mutations';
	import {
		buildAddColumnSql,
		buildAlterColumnSql,
		buildDropColumnSql,
		columnSpecFromInfo,
		emptyColumnSpec,
		runSqlStatements,
		type ColumnSpec
	} from '$lib/sql/column-mutations';
	import ColumnEditorDialog from './ColumnEditorDialog.svelte';
	import DataGrid from './DataGrid.svelte';
	import RecordFormView from './RecordFormView.svelte';
	import RowEditorDialog from './RowEditorDialog.svelte';
	import StructureGrid from './StructureGrid.svelte';
	import { t } from '$lib/i18n/i18n.svelte';

	let {
		connectionId,
		schema,
		name,
		isView
	}: {
		connectionId: string;
		schema: string;
		name: string;
		isView: boolean;
	} = $props();

	let mode = $state<'data' | 'structure' | 'ddl'>('data');
	let dataView = $state<'grid' | 'form'>('grid');
	let columns = $state<ColumnInfo[]>([]);
	let result = $state<QueryResult | null>(null);
	let ddl = $state('');
	let error = $state<string | null>(null);
	let success = $state<string | null>(null);
	let page = $state(0);
	let pageSize = $state(100);
	let loading = $state(false);
	let requestId = 0;
	let selectedRow = $state(0);
	let selectedColumn = $state(0);
	let mutating = $state(false);
	let editorMode = $state<'insert' | 'edit' | null>(null);
	let editorValues = $state<Record<string, string>>({});
	let editorError = $state<string | null>(null);
	let editSourceRow = $state<Array<string | null> | null>(null);
	let confirmDeleteOpen = $state(false);
	let columnEditorMode = $state<'add' | 'edit' | null>(null);
	let columnEditorValues = $state<ColumnSpec>(emptyColumnSpec());
	let columnEditorError = $state<string | null>(null);
	let columnEditSource = $state<ColumnInfo | null>(null);
	let confirmDropColumnOpen = $state(false);
	let rowMenu = $state<{ x: number; y: number; index: number } | null>(null);
	let successTimer: ReturnType<typeof setTimeout> | null = null;

	const connection = $derived(workspace.connections.find((item) => item.id === connectionId));
	const engine = $derived(connection?.engine);
	const estimatedRows = $derived(
		workspace.schema[connectionId]?.tables[schema]?.find((item) => item.name === name)?.tableRows ??
			null
	);
	const canMutate = $derived(!isView);
	const hasSelection = $derived(!!result && result.rows.length > 0 && selectedRow < result.rows.length);
	const hasColumnSelection = $derived(columns.length > 0 && selectedColumn < columns.length);
	const selectedColumnInfo = $derived(hasColumnSelection ? columns[selectedColumn] : null);
	const editFormColumns = $derived.by(() => {
		const keys = primaryKeyColumns(columns);
		const editable = editableColumns(columns);
		return editable.length ? [...keys, ...editable] : insertableColumns(columns);
	});
	const formColumns = $derived(editorMode === 'edit' ? editFormColumns : insertableColumns(columns));
	const readonlyNames = $derived(
		editorMode === 'edit' && editableColumns(columns).length
			? primaryKeyColumns(columns).map((column) => column.name)
			: []
	);

	function cellValue(column: ColumnInfo, row: Array<string | null>): string {
		if (!result) return '';
		const index = result.columns.findIndex((item) => item.name === column.name);
		if (index < 0) return '';
		return toInputValue(temporalKind(column), row[index] ?? null);
	}

	async function loadData() {
		const id = ++requestId;
		loading = true;
		error = null;
		success = null;
		try {
			await afterPaint();
			const next = await api.previewTable(connectionId, schema, name, pageSize, page * pageSize);
			if (id !== requestId) return;
			result = next;
			selectedRow = Math.min(selectedRow, Math.max(0, next.rows.length - 1));
		} catch (err) {
			if (id !== requestId) return;
			error = errorMessage(err);
		} finally {
			if (id === requestId) loading = false;
		}
	}

	async function loadStructure() {
		try {
			await afterPaint();
			columns = await api.getColumns(connectionId, schema, name);
			selectedColumn = Math.min(selectedColumn, Math.max(0, columns.length - 1));
		} catch (err) {
			error = errorMessage(err);
		}
	}

	async function afterSchemaChange() {
		ddl = '';
		await loadStructure();
		workspace.notifyTableChanged(connectionId, schema, name);
		if (mode === 'ddl') void loadDdl();
	}

	function closeEditor() {
		editorMode = null;
		editorError = null;
		editSourceRow = null;
	}

	function closeColumnEditor() {
		columnEditorMode = null;
		columnEditorError = null;
		columnEditSource = null;
	}

	function openInsert() {
		editorError = null;
		editSourceRow = null;
		const next: Record<string, string> = {};
		for (const column of insertableColumns(columns)) next[column.name] = '';
		editorValues = next;
		editorMode = 'insert';
	}

	function openEdit() {
		if (!hasSelection || !result) return;
		editorError = null;
		const row = result.rows[selectedRow];
		editSourceRow = row;
		const next: Record<string, string> = {};
		for (const column of editFormColumns) next[column.name] = cellValue(column, row);
		editorValues = next;
		editorMode = 'edit';
	}

	function openAddColumn() {
		columnEditorError = null;
		columnEditSource = null;
		columnEditorValues = emptyColumnSpec();
		columnEditorMode = 'add';
	}

	function openEditColumn(index = selectedColumn) {
		const column = columns[index];
		if (!column) return;
		selectedColumn = index;
		columnEditorError = null;
		columnEditSource = column;
		columnEditorValues = columnSpecFromInfo(column);
		columnEditorMode = 'edit';
	}

	function askDropColumn() {
		if (!hasColumnSelection) return;
		confirmDropColumnOpen = true;
	}

	async function loadDdl() {
		try {
			await afterPaint();
			ddl = await api.getDdl(connectionId, schema, isView ? 'view' : 'table', name);
		} catch (err) {
			error = errorMessage(err);
		}
	}

	async function submitColumnEditor() {
		if (mutating || !columnEditorMode) return;
		mutating = true;
		columnEditorError = null;
		error = null;
		try {
			const statements =
				columnEditorMode === 'add'
					? buildAddColumnSql(engine, schema, name, columnEditorValues)
					: buildAlterColumnSql(engine, schema, name, columnEditSource!, columnEditorValues);
			if (!statements.length) {
				columnEditorError = t('structure.noChanges');
				return;
			}
			await afterPaint();
			await runSqlStatements(
				(sql) => api.executeSql(connectionId, sql, schema, 'structure'),
				statements
			);
			const message =
				columnEditorMode === 'add' ? t('structure.added') : t('structure.updated');
			closeColumnEditor();
			await afterSchemaChange();
			showSuccess(message);
		} catch (err) {
			columnEditorError = errorMessage(err);
		} finally {
			mutating = false;
		}
	}

	async function confirmDropColumn() {
		if (!selectedColumnInfo || mutating) return;
		mutating = true;
		error = null;
		confirmDropColumnOpen = false;
		try {
			const sql = buildDropColumnSql(engine, schema, name, selectedColumnInfo.name);
			await afterPaint();
			await api.executeSql(connectionId, sql, schema, 'structure');
			await afterSchemaChange();
			showSuccess(t('structure.dropped'));
		} catch (err) {
			error = errorMessage(err);
		} finally {
			mutating = false;
		}
	}

	async function submitEditor() {
		if (mutating || !editorMode) return;
		mutating = true;
		editorError = null;
		error = null;
		try {
			const sql =
				editorMode === 'insert'
					? buildInsertSql(engine, schema, name, columns, editorValues)
					: buildUpdateSql(
							engine,
							schema,
							name,
							columns,
							result!.columns,
							editSourceRow!,
							editorValues
						);
			await afterPaint();
			await api.executeSql(connectionId, sql, schema, 'table_edit');
			closeEditor();
			await loadData();
		} catch (err) {
			editorError = errorMessage(err);
		} finally {
			mutating = false;
		}
	}

	function askDelete() {
		if (!hasSelection || !result) return;
		confirmDeleteOpen = true;
	}

	function showSuccess(message: string) {
		success = message;
		workspace.error = null;
		workspace.status = message;
		if (successTimer) clearTimeout(successTimer);
		successTimer = setTimeout(() => {
			if (success === message) success = null;
			successTimer = null;
		}, 2500);
	}

	function openRowMenu(event: MouseEvent, index: number) {
		event.preventDefault();
		rowMenu = { x: event.clientX, y: event.clientY, index };
	}

	function closeRowMenu() {
		rowMenu = null;
	}

	async function copyRowToSql() {
		if (!result || rowMenu == null) return;
		const index = rowMenu.index;
		const row = result.rows[index];
		if (!row) {
			closeRowMenu();
			return;
		}
		closeRowMenu();
		try {
			const sql = buildInsertSqlFromRow(engine, schema, name, result.columns, row);
			await navigator.clipboard.writeText(sql);
			error = null;
			showSuccess(t('table.copiedInsert'));
		} catch (err) {
			success = null;
			error = errorMessage(err);
		}
	}

	async function confirmDelete() {
		if (!hasSelection || !result || mutating) return;
		mutating = true;
		error = null;
		confirmDeleteOpen = false;
		try {
			const row = result.rows[selectedRow];
			const sql = buildDeleteSql(engine, schema, name, columns, result.columns, row);
			await afterPaint();
			await api.executeSql(connectionId, sql, schema, 'table_edit');
			await loadData();
		} catch (err) {
			error = errorMessage(err);
		} finally {
			mutating = false;
		}
	}

	function refreshData() {
		void loadData();
	}

	function refreshStructure() {
		void loadStructure();
	}

	$effect(() => {
		void connectionId;
		void schema;
		void name;
		void page;
		void pageSize;
		void workspace.tableDataRevision(connectionId, schema, name);
		void loadData();
	});

	$effect(() => {
		void connectionId;
		void schema;
		void name;
		void loadStructure();
	});
</script>

<div class="object-list">
	<div class="subtabs">
		<button class="subtab" class:active={mode === 'data'} onclick={() => (mode = 'data')}
			>{t('table.data')}</button
		>
		<button class="subtab" class:active={mode === 'structure'} onclick={() => (mode = 'structure')}
			>{t('table.structure')}</button
		>
		<button
			class="subtab"
			class:active={mode === 'ddl'}
			onclick={() => {
				mode = 'ddl';
				if (!ddl) void loadDdl();
			}}>{t('table.ddl')}</button
		>
	</div>
	{#if error}
		<div class="message error">{error}</div>
	{:else if success}
		<div class="message success">{success}</div>
	{/if}
	{#if mode === 'data'}
		<div class="filter-bar">
			<button class="btn" disabled={page === 0} onclick={() => (page = Math.max(0, page - 1))}
				>{t('table.prev')}</button
			>
			<button class="btn" onclick={() => (page += 1)}>{t('table.next')}</button>
			<button class="btn" disabled={loading} onclick={refreshData} title={t('table.refreshTitle')}
				>{t('table.refresh')}</button
			>
			<span>{t('table.page', { page: page + 1, size: pageSize })}</span>
			{#if estimatedRows != null}
				<span>~{formatNumber(estimatedRows)}</span>
			{/if}
			{#if result}
				<span>{formatDuration(result.durationMs)}</span>
			{/if}
			{#if loading}<span>{t('table.loading')}</span>{/if}
			<div class="view-mode-switch" role="group" aria-label={t('table.viewMode')}>
				<button
					class="btn"
					class:active={dataView === 'grid'}
					type="button"
					title={t('table.gridTitle')}
					onclick={() => (dataView = 'grid')}>{t('table.grid')}</button
				>
				<button
					class="btn"
					class:active={dataView === 'form'}
					type="button"
					title={t('table.formTitle')}
					onclick={() => (dataView = 'form')}>{t('table.form')}</button
				>
			</div>
			{#if canMutate}
				<div class="filter-bar-actions">
					<button class="btn" disabled={mutating} onclick={openInsert}>{t('table.insert')}</button>
					<button class="btn" disabled={mutating || !hasSelection} onclick={openEdit}
						>{t('table.edit')}</button
					>
					<button class="btn danger" disabled={mutating || !hasSelection} onclick={askDelete}
						>{t('table.delete')}</button
					>
				</div>
			{/if}
		</div>
		{#if result}
			{#if dataView === 'grid'}
				<DataGrid {result} bind:selectedRow onRowContextMenu={openRowMenu} />
			{:else}
				<RecordFormView {result} bind:selectedRow onRowContextMenu={openRowMenu} />
			{/if}
		{:else}
			<div class="empty">
				{loading ? t('table.loadingData') : error ? t('table.loadFailed') : t('table.noData')}
			</div>
		{/if}
	{:else if mode === 'structure'}
		<div class="filter-bar">
			<button
				class="btn"
				disabled={mutating}
				onclick={refreshStructure}
				title={t('structure.refreshTitle')}>{t('structure.refresh')}</button
			>
			<span>{t('structure.columnsCount', { count: columns.length })}</span>
			{#if canMutate}
				<div class="filter-bar-actions">
					<button class="btn" disabled={mutating} onclick={openAddColumn}>{t('structure.add')}</button>
					<button
						class="btn"
						disabled={mutating || !hasColumnSelection}
						onclick={() => openEditColumn()}>{t('structure.edit')}</button
					>
					<button
						class="btn danger"
						disabled={mutating || !hasColumnSelection}
						onclick={askDropColumn}>{t('structure.drop')}</button
					>
				</div>
			{/if}
		</div>
		<StructureGrid
			{columns}
			bind:selectedIndex={selectedColumn}
			onRowDblClick={canMutate ? (index) => openEditColumn(index) : undefined}
		/>
	{:else}
		<pre class="ddl-view">{ddl || t('table.loadingDdl')}</pre>
	{/if}
</div>

{#if editorMode}
	<RowEditorDialog
		title={editorMode === 'insert'
			? t('table.insertTitle', { name })
			: t('table.editTitle', { name })}
		hint={editorMode === 'insert' ? t('table.insertHint') : t('table.editHint')}
		columns={formColumns}
		bind:values={editorValues}
		{readonlyNames}
		error={editorError}
		pending={mutating}
		submitLabel={editorMode === 'insert' ? t('table.insertSubmit') : t('table.save')}
		pendingLabel={editorMode === 'insert' ? t('table.inserting') : t('table.saving')}
		onCancel={closeEditor}
		onSubmit={submitEditor}
	/>
{/if}

{#if columnEditorMode}
	<ColumnEditorDialog
		title={columnEditorMode === 'add'
			? t('structure.addTitle', { name })
			: t('structure.editTitle', { name })}
		hint={columnEditorMode === 'add' ? t('structure.addHint') : t('structure.editHint')}
		bind:values={columnEditorValues}
		error={columnEditorError}
		pending={mutating}
		submitLabel={columnEditorMode === 'add' ? t('structure.add') : t('table.save')}
		pendingLabel={t('structure.saving')}
		onCancel={closeColumnEditor}
		onSubmit={submitColumnEditor}
	/>
{/if}

{#if confirmDeleteOpen && result && hasSelection}
	<div class="modal-backdrop">
		<div class="modal" role="dialog" aria-modal="true" aria-labelledby="delete-row-title">
			<header id="delete-row-title">{t('table.deleteRowTitle')}</header>
			<div class="body">
				<p>
					{primaryKeyColumns(columns).length ? t('table.deleteRowPk') : t('table.deleteRowAll')}
				</p>
			</div>
			<footer>
				<button class="btn" onclick={() => (confirmDeleteOpen = false)} disabled={mutating}
					>{t('common.cancel')}</button
				>
				<button class="btn danger" onclick={() => void confirmDelete()} disabled={mutating}>
					{mutating ? t('table.deleting') : t('table.delete')}
				</button>
			</footer>
		</div>
	</div>
{/if}

{#if confirmDropColumnOpen && selectedColumnInfo}
	<div class="modal-backdrop">
		<div class="modal" role="dialog" aria-modal="true" aria-labelledby="drop-column-title">
			<header id="drop-column-title">{t('structure.dropTitle')}</header>
			<div class="body">
				<p>
					{t('structure.dropBody', { column: selectedColumnInfo.name, table: name })}
				</p>
			</div>
			<footer>
				<button class="btn" onclick={() => (confirmDropColumnOpen = false)} disabled={mutating}
					>{t('common.cancel')}</button
				>
				<button class="btn danger" onclick={() => void confirmDropColumn()} disabled={mutating}>
					{mutating ? t('structure.dropping') : t('structure.drop')}
				</button>
			</footer>
		</div>
	</div>
{/if}

{#if rowMenu}
	<div class="context-menu" style:left="{rowMenu.x}px" style:top="{rowMenu.y}px" role="menu">
		<button type="button" onclick={() => void copyRowToSql()}>{t('table.copyToSql')}</button>
	</div>
	<button
		class="modal-backdrop"
		style="background:transparent"
		onclick={closeRowMenu}
		aria-label={t('table.closeMenu')}
	></button>
{/if}
