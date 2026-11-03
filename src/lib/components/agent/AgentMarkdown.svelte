<script lang="ts">
	import type { ChartEngine } from '$lib/agent/chart-spec';
	import { renderRichSegmentHtml, splitRichContent } from '$lib/agent/rich-content';
	import { appMode } from '$lib/stores/app-mode.svelte';
	import AgentChart from './AgentChart.svelte';

	let {
		content = '',
		engine,
		variant = 'assistant'
	}: {
		content?: string;
		engine?: ChartEngine;
		/** Visual tone: assistant body or compact user bubble. */
		variant?: 'assistant' | 'user';
	} = $props();

	const chartEngine = $derived(engine ?? appMode.prefs.chartEngine);
	const segments = $derived.by(() => splitRichContent(content ?? ''));
</script>

<div class="agent-md" class:user={variant === 'user'} class:assistant={variant === 'assistant'}>
	{#each segments as segment, index (index)}
		{#if segment.kind === 'chart'}
			{#if variant === 'assistant'}
				<AgentChart spec={segment.spec} engine={chartEngine} />
			{:else}
				<pre class="agent-md-chart-fallback">{segment.raw}</pre>
			{/if}
		{:else}
			{@html renderRichSegmentHtml(segment)}
		{/if}
	{/each}
</div>
