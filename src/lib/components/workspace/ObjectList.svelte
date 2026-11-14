<script lang="ts">
	import {
		workspace,
		folderLabel,
		folderToKind,
		isSchemaObjectFolder,
		schemaObjectsKey
	} from '$lib/stores/workspace.svelte';
	import { formatBytes, formatNumber } from '$lib/format';
	import type { FolderKind } from '$lib/api/types';
	import { t, type MessageKey } from '$lib/i18n/i18n.svelte';

	let {
		connectionId,
		schema,
		folder
	}: {
		connectionId: string;
		schema: string;
		folder: FolderKind;
	} = $props();

	let filter = $state('');
	const cache = $derived(workspace.schema[connectionId]);
	const loading = $derived(
		workspace.isPending(workspace.folderJobKey(connectionId, schema, folder))
	);

	const yes = $derived(t('objects.yes'));
	const no = $derived(t('objects.no'));

	const rows = $derived.by(() => {
		const q = filter.trim().toLowerCase();
		if (folder === 'tables') {
			return (cache?.tables[schema] ?? [])
				.filter((item) => item.name.toLowerCase().includes(q))
				.map((item) => ({
					key: item.name,
					cells: [
						item.name,
						item.engine ?? '—',
						formatNumber(item.tableRows),
						formatBytes(item.dataLength),
						item.comment || '—',
						item.createdAt ?? '—'
					]
				}));
		}
		if (folder === 'views') {
			return (cache?.views[schema] ?? [])
				.filter((item) => item.name.toLowerCase().includes(q))
				.map((item) => ({
					key: item.name,
					cells: [
						item.name,
						item.updatable ? yes : no,
						item.securityType ?? '—',
						item.definer ?? '—'
					]
				}));
		}
		if (folder === 'indexes') {
			return (cache?.indexes[schema] ?? [])
				.filter((item) => `${item.tableName}.${item.name}`.toLowerCase().includes(q))
				.map((item) => ({
					key: `${item.tableName}.${item.name}`,
					cells: [
						item.name,
						item.tableName,
						item.primary ? 'PRIMARY' : item.unique ? 'UNIQUE' : 'INDEX',
						item.indexType,
						item.columns.join(', ')
					]
				}));
		}
		if (isSchemaObjectFolder(folder)) {
			return (cache?.objects[schemaObjectsKey(schema, folder)] ?? [])
				.filter((item) => item.name.toLowerCase().includes(q))
				.map((item) => ({ key: item.name, cells: [item.name, item.detail || '—'] }));
		}
		if (folder === 'triggers') {
			return (cache?.triggers[schema] ?? [])
				.filter((item) => item.name.toLowerCase().includes(q))
				.map((item) => ({
					key: item.name,
					cells: [item.name, item.tableName, item.timing, item.event, item.definer ?? '—']
				}));
		}
		return (cache?.routines[schema] ?? [])
			.filter((item) => item.name.toLowerCase().includes(q))
			.map((item) => ({
				key: item.name,
				cells: [
					item.name,
					item.routineType,
					item.returns ?? '—',
					item.deterministic ? yes : no,
					item.definer ?? '—'
				]
			}));
	});

	const headers = $derived.by((): MessageKey[] => {
		if (folder === 'tables') {
			return [
				'objects.col.name',
				'objects.col.engine',
				'objects.col.rows',
				'objects.col.size',
				'objects.col.comment',
				'objects.col.created'
			];
		}
		if (folder === 'views') {
			return [
				'objects.col.name',
				'objects.col.updatable',
				'objects.col.security',
				'objects.col.definer'
			];
		}
		if (folder === 'indexes') {
			return [
				'objects.col.name',
				'objects.col.table',
				'objects.col.kind',
				'objects.col.type',
				'objects.col.columns'
			];
		}
		if (isSchemaObjectFolder(folder)) return ['objects.col.name', 'objects.col.detail'];
		if (folder === 'triggers') {
			return [
				'objects.col.name',
				'objects.col.table',
				'objects.col.timing',
				'objects.col.event',
				'objects.col.definer'
			];
		}
		return [
			'objects.col.name',
			'objects.col.type',
			'objects.col.returns',
			'objects.col.deterministic',
			'objects.col.definer'
		];
	});

	function open(name: string) {
		if (folder === 'tables') workspace.openTable(connectionId, schema, name, false);
		else if (folder === 'views') workspace.openTable(connectionId, schema, name, true);
		else if (folder === 'indexes') return;
		else if (isSchemaObjectFolder(folder))
			workspace.openDdl(connectionId, schema, folderToKind(folder), name);
		else if (folder === 'functions') {
			const routine = cache?.routines[schema]?.find((item) => item.name === name);
			workspace.openDdl(
				connectionId,
				schema,
				routine?.routineType === 'PROCEDURE' ? 'procedure' : 'function',
				name
			);
		} else workspace.openDdl(connectionId, schema, 'trigger', name);
	}
</script>

<div class="object-list">
	<div class="filter-bar">
		<strong>{folderLabel(folder)}</strong>
		<span style="color:var(--text-muted)">{schema}</span>
		<input placeholder={t('objects.filter')} bind:value={filter} />
	</div>
	<div class="data-grid-wrap">
		<table class="data-grid">
			<thead>
				<tr>
					{#each headers as header (header)}
						<th>{t(header)}</th>
					{/each}
				</tr>
			</thead>
			<tbody>
				{#each rows as row (row.key)}
					<tr ondblclick={() => open(row.cells[0])}>
						{#each row.cells as cell, index (`${row.key}-${index}`)}
							<td>{cell}</td>
						{/each}
					</tr>
				{/each}
			</tbody>
		</table>
		{#if rows.length === 0}
			<div class="empty">{loading ? t('objects.loading') : t('objects.empty')}</div>
		{/if}
	</div>
</div>
