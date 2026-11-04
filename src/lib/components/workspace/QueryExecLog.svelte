<script module lang="ts">
	const STORAGE_KEY = 'hidatalinker.queryLog';
	const DEFAULT_HEIGHT = 140;

	function loadPrefs(): { height: number; collapsed: boolean } {
		try {
			const raw = localStorage.getItem(STORAGE_KEY);
			if (raw) {
				const parsed = JSON.parse(raw) as { height?: unknown; collapsed?: unknown };
				return {
					height: typeof parsed.height === 'number' ? parsed.height : DEFAULT_HEIGHT,
					collapsed: parsed.collapsed === true
				};
			}
		} catch {
			// ignore unavailable storage
		}
		return { height: DEFAULT_HEIGHT, collapsed: false };
	}

	const prefs = loadPrefs();

	function savePrefs() {
		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
		} catch {
			// ignore unavailable storage
		}
	}
</script>

<script lang="ts">
	import { ChevronDown, ChevronUp } from '@lucide/svelte';
	import type { QueryLogEntry } from '$lib/api/types';
	import { t } from '$lib/i18n/i18n.svelte';

	let { entries }: { entries: QueryLogEntry[] } = $props();

	const TITLE_PX = 28;
	const MIN_BODY_PX = 48;
	const MIN_RESULTS_PX = 72;

	let root = $state<HTMLDivElement | undefined>(undefined);
	let height = $state(prefs.height);
	let collapsed = $state(prefs.collapsed);
	let dragging = $state(false);
	let dragStartY = 0;
	let dragStartHeight = 0;

	const lastEntry = $derived(entries.at(-1));

	function maxHeight(): number {
		const parent = root?.parentElement;
		if (!parent) return 600;
		return Math.max(MIN_BODY_PX, parent.clientHeight - MIN_RESULTS_PX - TITLE_PX);
	}

	function persist() {
		prefs.height = height;
		prefs.collapsed = collapsed;
		savePrefs();
	}

	function toggle() {
		collapsed = !collapsed;
		if (!collapsed) height = Math.min(Math.max(height, MIN_BODY_PX), maxHeight());
		persist();
	}

	function start(event: PointerEvent) {
		event.preventDefault();
		dragging = true;
		dragStartY = event.clientY;
		dragStartHeight = collapsed ? 0 : height;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
	}

	function move(event: PointerEvent) {
		if (!dragging) return;
		const next = dragStartHeight + (dragStartY - event.clientY);
		if (next < MIN_BODY_PX / 2) {
			collapsed = true;
			return;
		}
		collapsed = false;
		height = Math.min(maxHeight(), Math.max(MIN_BODY_PX, next));
	}

	function end() {
		if (!dragging) return;
		dragging = false;
		persist();
	}
</script>

<div
	class="query-exec-log"
	class:collapsed
	class:dragging
	bind:this={root}
	aria-label={t('query.log.title')}
>
	<div
		class="query-exec-log-handle"
		class:active={dragging}
		onpointerdown={start}
		onpointermove={move}
		onpointerup={end}
		onpointercancel={end}
		onlostpointercapture={end}
		role="separator"
		aria-orientation="horizontal"
		aria-label={t('split.resize')}
	></div>
	<button
		type="button"
		class="query-exec-log-title"
		onclick={toggle}
		aria-expanded={!collapsed}
		title={collapsed ? t('query.log.show') : t('query.log.hide')}
	>
		<span class="query-exec-log-toggle" aria-hidden="true">
			{#if collapsed}<ChevronUp size={13} />{:else}<ChevronDown size={13} />{/if}
		</span>
		<span>{t('query.log.title')}</span>
		{#if collapsed && lastEntry}
			<span class="query-exec-log-peek level-{lastEntry.level}">{lastEntry.text}</span>
		{/if}
	</button>
	{#if !collapsed}
		<ol class="query-exec-log-list" style:height="{height}px">
			{#each entries as entry, index (index)}
				<li class="query-exec-log-item level-{entry.level}">
					<span class="query-exec-log-level">{entry.level}</span>
					<span class="query-exec-log-text">{entry.text}</span>
				</li>
			{/each}
		</ol>
	{/if}
</div>
