<script lang="ts" module>
	let savedHeight = 180;
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { QueryResult } from '$lib/api/types';
	import { applyFieldCodecs, type CodecRuleProblem } from '$lib/sql/field-codec';
	import { workspace } from '$lib/stores/workspace.svelte';
	import { t } from '$lib/i18n/i18n.svelte';
	import FieldCodecBar from './FieldCodecBar.svelte';

	let {
		tabId,
		result,
		open = $bindable(false),
		display = $bindable<QueryResult | null>(null),
		activeColumns = $bindable<number[]>([]),
		failedCells = $bindable(0),
		children
	}: {
		tabId: string;
		result: QueryResult;
		open?: boolean;
		display?: QueryResult | null;
		activeColumns?: number[];
		failedCells?: number;
		children: Snippet<[QueryResult, number[]]>;
	} = $props();

	const rules = $derived(workspace.fieldCodecs[tabId] ?? []);
	let height = $state(savedHeight);
	let dragging = $state(false);
	let root = $state<HTMLDivElement | null>(null);
	let dragOrigin = { y: 0, height: 0 };
	let issues = $state<CodecRuleProblem[]>([]);

	const shown = $derived(display ?? result);

	$effect(() => {
		const source = result;
		const current = rules;
		if (!source.columns.length || current.length === 0) {
			display = source;
			activeColumns = [];
			failedCells = 0;
			issues = [];
			return;
		}
		let cancelled = false;
		display = source;
		activeColumns = [];
		failedCells = 0;
		void applyFieldCodecs(source.columns, source.rows, current).then((applied) => {
			if (cancelled) return;
			display = applied.rows === source.rows ? source : { ...source, rows: applied.rows };
			activeColumns = applied.activeColumns;
			failedCells = applied.failedCells;
			issues = applied.issues;
		});
		return () => {
			cancelled = true;
		};
	});

	function startDrag(event: PointerEvent) {
		dragging = true;
		dragOrigin = { y: event.clientY, height };
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
	}

	function moveDrag(event: PointerEvent) {
		if (!dragging || !root) return;
		const max = Math.max(120, root.clientHeight - 96);
		height = Math.min(max, Math.max(88, dragOrigin.height + (event.clientY - dragOrigin.y)));
		savedHeight = height;
	}

	function endDrag() {
		dragging = false;
	}
</script>

<div class="codec-results" class:dragging bind:this={root}>
	{#if open}
		<div class="codec-results-pane" style:height="{height}px">
			<FieldCodecBar
				{rules}
				columns={result.columns.map((column) => column.name)}
				{issues}
				onChange={(next) => workspace.setFieldCodecs(tabId, next)}
			/>
		</div>
		<div
			class="split-handle horizontal"
			class:active={dragging}
			role="separator"
			aria-orientation="horizontal"
			aria-label={t('codec.resize')}
			onpointerdown={startDrag}
			onpointermove={moveDrag}
			onpointerup={endDrag}
			onpointercancel={endDrag}
			onlostpointercapture={endDrag}
		></div>
	{/if}
	<div class="codec-results-body">
		{@render children(shown, activeColumns)}
	</div>
</div>

<style>
	.codec-results {
		display: flex;
		flex: 1 1 auto;
		flex-direction: column;
		width: 100%;
		min-width: 0;
		min-height: 0;
	}

	.codec-results-pane {
		display: flex;
		flex: none;
		min-height: 0;
		overflow: hidden;
	}

	.codec-results-pane :global(.codec-panel) {
		max-height: none;
	}

	.codec-results-body {
		display: flex;
		flex: 1 1 auto;
		flex-direction: column;
		min-width: 0;
		min-height: 72px;
		overflow: hidden;
	}

	.codec-results-body > :global(.data-grid-wrap),
	.codec-results-body > :global(.record-form-view) {
		flex: 1 1 auto;
		min-height: 0;
	}
</style>
