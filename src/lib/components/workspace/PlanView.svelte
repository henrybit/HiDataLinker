<script lang="ts">
	import { ChevronDown, ChevronRight } from '@lucide/svelte';
	import { formatNumber } from '$lib/format';
	import { t } from '$lib/i18n/i18n.svelte';
	import { nodeShare, type ParsedPlan, type PlanNode } from '$lib/sql/pg-plan';

	let { plan, raw }: { plan: ParsedPlan; raw: string } = $props();

	let view = $state<'tree' | 'json'>('tree');
	let collapsed = $state<Set<number>>(new Set());
	let showOutput = $state(false);

	const visible = $derived.by(() => {
		const out: PlanNode[] = [];
		const walk = (node: PlanNode) => {
			out.push(node);
			if (collapsed.has(node.id)) return;
			for (const child of node.children) walk(child);
		};
		walk(plan.root);
		return out;
	});
	const misestimates = $derived(plan.nodes.filter((node) => node.misestimated).length);

	function toggle(id: number) {
		const next = new Set(collapsed);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		collapsed = next;
	}

	function ms(value: number | null): string {
		if (value == null) return '—';
		return value >= 100 ? `${value.toFixed(0)} ms` : `${value.toFixed(value >= 10 ? 1 : 2)} ms`;
	}

	function cost(value: number): string {
		return value.toFixed(2);
	}

	function percent(node: PlanNode): number {
		return Math.round(nodeShare(plan, node) * 1000) / 10;
	}

	function factorLabel(node: PlanNode): string {
		const factor = node.estimateFactor ?? 1;
		const value = factor >= 100 ? factor.toFixed(0) : factor.toFixed(1);
		return node.underestimated
			? t('plan.underestimated', { factor: value })
			: t('plan.overestimated', { factor: value });
	}
</script>

<div class="plan-view">
	<div class="plan-toolbar">
		<div class="plan-summary">
			{#if plan.analyzed}
				<span>{t('plan.planning')} {ms(plan.planningTime)}</span>
				<span>{t('plan.execution')} {ms(plan.executionTime)}</span>
			{:else}
				<span>{t('plan.totalCost')} {cost(plan.root.totalCost)}</span>
				<span class="muted">{t('plan.estimateOnly')}</span>
			{/if}
			<span>{t('plan.nodes', { count: plan.nodes.length })}</span>
			{#if misestimates}
				<span class="badge warn">{t('plan.misestimateCount', { count: misestimates })}</span>
			{/if}
		</div>
		<label class="plan-check">
			<input type="checkbox" bind:checked={showOutput} />
			{t('plan.showOutput')}
		</label>
		<div class="plan-tabs" role="tablist">
			<button
				type="button"
				class:active={view === 'tree'}
				role="tab"
				aria-selected={view === 'tree'}
				onclick={() => (view = 'tree')}>{t('plan.tree')}</button
			>
			<button
				type="button"
				class:active={view === 'json'}
				role="tab"
				aria-selected={view === 'json'}
				onclick={() => (view = 'json')}>JSON</button
			>
		</div>
	</div>

	{#if view === 'json'}
		<pre class="plan-json">{raw}</pre>
	{:else}
		<div class="plan-table" role="tree">
			<div class="plan-row plan-head">
				<span>{t('plan.node')}</span>
				<span>{plan.analyzed ? t('plan.selfTime') : t('plan.selfCost')}</span>
				<span>{t('plan.rows')}</span>
				<span>{t('plan.cost')}</span>
				{#if plan.analyzed}<span>{t('plan.buffers')}</span>{/if}
			</div>
			{#each visible as node (node.id)}
				{@const share = percent(node)}
				<div
					class="plan-row"
					class:hot={node.id === plan.hottestId}
					role="treeitem"
					aria-selected="false"
					aria-expanded={node.children.length ? !collapsed.has(node.id) : undefined}
				>
					<div class="plan-node" style:padding-left="{node.depth * 16}px">
						{#if node.children.length}
							<button type="button" class="chevron" onclick={() => toggle(node.id)}>
								{#if collapsed.has(node.id)}<ChevronRight size={12} />{:else}<ChevronDown
										size={12}
									/>{/if}
							</button>
						{:else}
							<span class="chevron"></span>
						{/if}
						<div class="plan-node-body">
							<div class="plan-title">
								<span>{node.title}</span>
								{#if node.id === plan.hottestId}
									<span class="badge hot"
										>{plan.analyzed ? t('plan.slowest') : t('plan.costliest')}</span
									>
								{/if}
								{#if node.misestimated}
									<span class="badge warn">{factorLabel(node)}</span>
								{/if}
							</div>
							{#each node.details as detail (detail)}
								<div class="plan-detail">{detail}</div>
							{/each}
							{#if showOutput && node.output.length}
								<div class="plan-detail">Output: {node.output.join(', ')}</div>
							{/if}
						</div>
					</div>
					<div class="plan-metric">
						<span class="share-bar"><span style:width="{Math.min(share, 100)}%"></span></span>
						<span
							>{plan.analyzed ? ms(node.exclusiveTime) : cost(node.exclusiveCost)} · {share}%</span
						>
					</div>
					<div class="plan-metric">
						{#if node.actualRows != null}
							<span>{formatNumber(node.actualRows)}</span>
							<span class="muted">/ {formatNumber(node.planRows)}</span>
							{#if (node.actualLoops ?? 1) > 1}
								<span class="muted">× {formatNumber(node.actualLoops)}</span>
							{/if}
						{:else}
							<span>{formatNumber(node.planRows)}</span>
						{/if}
					</div>
					<div class="plan-metric">{cost(node.startupCost)}..{cost(node.totalCost)}</div>
					{#if plan.analyzed}
						<div class="plan-metric">
							{#if node.buffers}
								<span title={t('plan.buffersHint')}
									>{formatNumber(node.buffers.sharedHit)} / {formatNumber(
										node.buffers.sharedRead
									)}</span
								>
								{#if node.buffers.tempRead || node.buffers.tempWritten}
									<span class="badge warn"
										>temp {formatNumber(node.buffers.tempRead + node.buffers.tempWritten)}</span
									>
								{/if}
							{:else}
								—
							{/if}
						</div>
					{/if}
				</div>
			{/each}
			{#if plan.triggers.length}
				<div class="plan-triggers">
					{#each plan.triggers as trigger (trigger.name)}
						<div>
							{t('plan.trigger', {
								name: trigger.name,
								calls: trigger.calls,
								time: ms(trigger.time)
							})}
						</div>
					{/each}
				</div>
			{/if}
			{#if plan.analyzed}
				<div class="plan-footnote">{t('plan.rolledBack')}</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.plan-view {
		display: flex;
		flex: 1 1 0;
		min-height: 0;
		flex-direction: column;
	}

	.plan-toolbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px;
		padding: 6px 10px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-muted);
		font-size: 12px;
	}

	.plan-summary {
		display: flex;
		flex: 1 1 260px;
		flex-wrap: wrap;
		gap: 12px;
		align-items: center;
	}

	.plan-check {
		display: flex;
		align-items: center;
		gap: 4px;
		color: var(--text-muted);
	}

	.plan-tabs {
		display: flex;
		border: 1px solid var(--border);
		border-radius: 4px;
		overflow: hidden;
	}

	.plan-tabs button {
		padding: 2px 10px;
		border: 0;
		background: var(--bg-panel);
		font-size: 12px;
		cursor: pointer;
	}

	.plan-tabs button.active {
		background: var(--bg-selected);
	}

	.plan-json {
		flex: 1;
		margin: 0;
		padding: 10px;
		overflow: auto;
		font-family: var(--mono);
		font-size: 12px;
	}

	.plan-table {
		flex: 1;
		overflow: auto;
		font-size: 12px;
	}

	.plan-row {
		display: grid;
		grid-template-columns: minmax(240px, 1fr) 150px 130px 120px 100px;
		min-width: 780px;
		gap: 8px;
		padding: 4px 10px;
		border-bottom: 1px solid var(--bg-muted);
		align-items: start;
	}

	.plan-head {
		position: sticky;
		top: 0;
		z-index: 1;
		background: var(--bg-panel);
		color: var(--text-muted);
		font-weight: 600;
		border-bottom: 1px solid var(--border);
	}

	.plan-row.hot {
		background: #fef2f2;
	}

	.plan-node {
		display: flex;
		gap: 4px;
		min-width: 0;
	}

	.chevron {
		display: inline-flex;
		flex: none;
		width: 14px;
		height: 16px;
		padding: 0;
		border: 0;
		background: none;
		align-items: center;
		cursor: pointer;
		color: var(--text-muted);
	}

	.plan-node-body {
		min-width: 0;
	}

	.plan-title {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		align-items: center;
		font-weight: 600;
	}

	.plan-detail {
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11px;
		word-break: break-word;
	}

	.plan-metric {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		align-items: center;
		font-variant-numeric: tabular-nums;
	}

	.share-bar {
		display: inline-block;
		width: 40px;
		height: 6px;
		border-radius: 3px;
		background: var(--bg-muted);
		overflow: hidden;
	}

	.share-bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}

	.plan-row.hot .share-bar span {
		background: var(--danger);
	}

	.badge {
		padding: 0 5px;
		border-radius: 3px;
		font-size: 11px;
		font-weight: 500;
	}

	.badge.hot {
		background: var(--danger);
		color: #fff;
	}

	.badge.warn {
		background: #fef3c7;
		color: var(--warning);
	}

	.muted {
		color: var(--text-muted);
	}

	.plan-triggers,
	.plan-footnote {
		padding: 6px 10px;
		color: var(--text-muted);
	}
</style>
