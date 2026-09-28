<script lang="ts">
	import { cardinalityLabel, layoutNodes } from '$lib/analysis/layout';
	import type { CatalogObject, RelationshipEdge } from '$lib/analysis/types';
	import { t } from '$lib/i18n/i18n.svelte';

	let {
		nodes,
		edges,
		selectedId = $bindable(null)
	}: {
		nodes: CatalogObject[];
		edges: RelationshipEdge[];
		selectedId?: string | null;
	} = $props();

	let positions = $state<Record<string, { x: number; y: number }>>({});
	let scale = $state(1);
	const layoutKey = $derived(
		`${nodes.map((node) => node.id).join('|')}::${edges.map((edge) => edge.id).join('|')}`
	);

	$effect(() => {
		void layoutKey;
		positions = layoutNodes(
			nodes.map((node) => ({ id: node.id, group: `${node.connectionId}:${node.schema}` })),
			edges.map((edge) => ({ fromId: edge.fromId, toId: edge.toId }))
		);
	});

	const bounds = $derived.by(() => {
		const values = Object.values(positions);
		if (values.length === 0) return { width: 640, height: 320 };
		return {
			width: Math.max(...values.map((point) => point.x)) + 180,
			height: Math.max(...values.map((point) => point.y)) + 90
		};
	});

	function edgeColor(edge: RelationshipEdge): string {
		if (edge.origin === 'view') return '#0f766e';
		if (edge.origin === 'inferred') return edge.strength === 'weak' ? '#b45309' : '#15803d';
		return '#334155';
	}

	function nodeFill(node: CatalogObject): string {
		if (node.external) return '#64748b';
		return node.kind === 'view' ? '#7c3aed' : '#2563eb';
	}

	function shortName(name: string): string {
		return name.length > 22 ? `${name.slice(0, 20)}…` : name;
	}

	function pointOf(id: string) {
		return positions[id];
	}

	function onWheel(event: WheelEvent) {
		event.preventDefault();
		const next = scale * (event.deltaY > 0 ? 0.9 : 1.1);
		scale = Math.min(2.4, Math.max(0.45, next));
	}

	function wheelZoom(node: HTMLElement) {
		node.addEventListener('wheel', onWheel, { passive: false });
		return {
			destroy() {
				node.removeEventListener('wheel', onWheel);
			}
		};
	}

	function dragNode(event: PointerEvent, id: string) {
		selectedId = id;
		const origin = positions[id];
		if (!origin) return;
		const startX = event.clientX;
		const startY = event.clientY;
		const move = (ev: PointerEvent) => {
			positions[id] = {
				x: origin.x + (ev.clientX - startX) / scale,
				y: origin.y + (ev.clientY - startY) / scale
			};
		};
		const up = () => {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
	}
</script>

<div class="analysis-canvas" use:wheelZoom>
	<button class="btn analysis-fit" type="button" onclick={() => (scale = 1)}
		>{t('analysis.fit')}</button
	>
	<svg width={bounds.width * scale} height={bounds.height * scale} role="img">
		<g transform={`scale(${scale})`}>
			{#each edges as edge (edge.id)}
				{@const from = pointOf(edge.fromId)}
				{@const to = pointOf(edge.toId)}
				{#if from && to}
					<line
						x1={from.x}
						y1={from.y}
						x2={to.x}
						y2={to.y}
						stroke={edgeColor(edge)}
						stroke-width={edge.strength === 'weak' ? 1.4 : 2}
						stroke-dasharray={edge.strength === 'weak' ? '5 4' : '0'}
					/>
					<text
						x={(from.x + to.x) / 2}
						y={(from.y + to.y) / 2 - 6}
						text-anchor="middle"
						class="analysis-edge-label"
						fill={edgeColor(edge)}
					>
						{cardinalityLabel(edge.cardinality)}
					</text>
				{/if}
			{/each}
			{#each nodes as node (node.id)}
				{@const point = pointOf(node.id)}
				{#if point}
					<g
						class="analysis-node"
						role="button"
						aria-label={`${node.schema}.${node.name}`}
						tabindex="0"
						transform={`translate(${point.x} ${point.y})`}
						onpointerdown={(event) => dragNode(event, node.id)}
						onkeydown={(event) => {
							if (event.key === 'Enter' || event.key === ' ') selectedId = node.id;
						}}
					>
						<circle
							r="22"
							fill={nodeFill(node)}
							stroke={selectedId === node.id ? '#111827' : 'white'}
							stroke-width="2"
						/>
						<title>{node.schema}.{node.name}</title>
						<text y="38" text-anchor="middle">{shortName(node.name)}</text>
					</g>
				{/if}
			{/each}
		</g>
	</svg>
</div>
