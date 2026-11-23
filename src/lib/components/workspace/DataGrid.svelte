<script lang="ts">
	import type { QueryResult } from '$lib/api/types';
	import { browserTextMeasure, cellNeedsExpand, dataGridLayout } from '$lib/grid/column-widths';
	import { t } from '$lib/i18n/i18n.svelte';
	import CellDetailDrawer from './CellDetailDrawer.svelte';

	const ROW_HEIGHT = 26;
	const OVERSCAN = 12;

	let {
		result,
		selectedRow = $bindable(0),
		codecColumns = [],
		onRowContextMenu
	}: {
		result: QueryResult;
		selectedRow?: number;
		/** Column indexes whose displayed values were decoded or decrypted. */
		codecColumns?: number[];
		onRowContextMenu?: (event: MouseEvent, index: number) => void;
	} = $props();

	let scrollTop = $state(0);
	let viewportHeight = $state(360);
	let scroller = $state<HTMLDivElement | null>(null);
	let detail = $state<{
		columnName: string;
		columnType?: string;
		value: string;
	} | null>(null);

	const colCount = $derived(result.columns.length + 1);
	const codecSet = $derived(new Set(codecColumns));
	const layout = $derived.by(() =>
		dataGridLayout(result.columns, result.rows, t('grid.null'), browserTextMeasure())
	);
	const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
	const visibleCount = $derived(Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN * 2);
	const end = $derived(Math.min(result.rows.length, start + visibleCount));
	const visibleRows = $derived(result.rows.slice(start, end));
	const useWindow = $derived(result.rows.length > 80);

	$effect(() => {
		const element = scroller;
		if (!element) return;
		const syncHeight = () => {
			const next = element.clientHeight;
			if (next > 0) viewportHeight = next;
		};
		syncHeight();
		const observer = new ResizeObserver(syncHeight);
		observer.observe(element);
		return () => observer.disconnect();
	});

	function onScroll(event: Event) {
		const next = (event.currentTarget as HTMLElement).scrollTop;
		if (Math.floor(next / ROW_HEIGHT) === Math.floor(scrollTop / ROW_HEIGHT)) return;
		scrollTop = next;
	}

	function openDetail(event: MouseEvent, cellIndex: number, value: string) {
		event.stopPropagation();
		const column = result.columns[cellIndex];
		detail = {
			columnName: column?.name ?? t('grid.columnFallback', { n: cellIndex + 1 }),
			columnType: column?.typeName,
			value
		};
	}
</script>

<div class="data-grid-wrap" bind:this={scroller} onscroll={onScroll}>
	<table
		class="data-grid stable-cols"
		class:windowed={useWindow}
		style="width: {layout.tableWidth}px"
	>
		<colgroup>
			<col style="width: {layout.indexWidth}px" />
			{#each layout.columnWidths as width, index (`${index}:${result.columns[index]?.name ?? ''}`)}
				<col style="width: {width}px" />
			{/each}
		</colgroup>
		<thead>
			<tr>
				<th>#</th>
				{#each result.columns as column, index (`${index}:${column.name}`)}
					<th title={column.typeName}>
						{column.name}
						{#if codecSet.has(index)}
							<span class="codec-mark">{t('codec.mark')}</span>
						{/if}
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#if useWindow && start > 0}
				<tr class="spacer-row" aria-hidden="true">
					<td colspan={colCount} style="height:{start * ROW_HEIGHT}px;padding:0;border:none"></td>
				</tr>
			{/if}
			{#each visibleRows as row, offset (`${start + offset}`)}
				{@const index = useWindow ? start + offset : offset}
				<tr
					class:selected={selectedRow === index}
					class:stripe={useWindow && index % 2 === 1}
					onclick={() => (selectedRow = index)}
					oncontextmenu={(event) => {
						selectedRow = index;
						onRowContextMenu?.(event, index);
					}}
					onkeydown={(event) => event.key === 'Enter' && (selectedRow = index)}
					tabindex="0"
				>
					<td>{index + 1}</td>
					{#each row as cell, cellIndex (`${index}-${cellIndex}`)}
						<td title={cell ?? t('input.null')}>
							{#if cell == null}
								<span class="null-cell">{t('grid.null')}</span>
							{:else if cellNeedsExpand(cell)}
								<span class="cell-inner">
									<span class="cell-text">{cell}</span>
									<button
										class="cell-expand"
										type="button"
										title={t('grid.viewFull')}
										aria-label={t('grid.viewFull')}
										onclick={(event) => openDetail(event, cellIndex, cell)}
									>
										…
									</button>
								</span>
							{:else}
								{cell}
							{/if}
						</td>
					{/each}
				</tr>
			{/each}
			{#if useWindow && end < result.rows.length}
				<tr class="spacer-row" aria-hidden="true">
					<td
						colspan={colCount}
						style="height:{(result.rows.length - end) * ROW_HEIGHT}px;padding:0;border:none"
					></td>
				</tr>
			{/if}
		</tbody>
	</table>
	{#if result.rows.length === 0}
		<div class="empty" style="height:120px">{t('grid.noRows')}</div>
	{/if}
</div>

{#if detail}
	<CellDetailDrawer
		columnName={detail.columnName}
		columnType={detail.columnType}
		value={detail.value}
		onClose={() => (detail = null)}
	/>
{/if}
