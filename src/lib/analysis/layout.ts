import type { Cardinality, GraphFilters, RelationshipEdge } from './types';

export function cardinalityLabel(cardinality: Cardinality): string {
	switch (cardinality) {
		case 'one_to_one':
			return '1:1';
		case 'one_to_many':
			return '1:N';
		case 'many_to_one':
			return 'N:1';
		case 'many_to_many':
			return 'N:N';
	}
}

export function edgeVisible(edge: RelationshipEdge, filters: GraphFilters): boolean {
	const known = edge.origin === 'physical' || edge.origin === 'view';
	if (known && !filters.physical) return false;
	if (!known && !filters.inferred) return false;
	if (edge.strength === 'strong' && !filters.strong) return false;
	if (edge.strength === 'weak' && !filters.weak) return false;
	return true;
}

export function layoutNodes(
	nodes: Array<{ id: string; group: string }>,
	edges: Array<{ fromId: string; toId: string }>
): Record<string, { x: number; y: number }> {
	const positions: Record<string, { x: number; y: number }> = {};
	const groups = [...new Set(nodes.map((node) => node.group))];
	groups.forEach((group, column) => {
		nodes
			.filter((node) => node.group === group)
			.forEach((node, row) => {
				positions[node.id] = { x: 140 + column * 240, y: 72 + row * 96 };
			});
	});
	if (nodes.length > 400) return positions;

	const ids = nodes.map((node) => node.id);
	for (let step = 0; step < 24; step += 1) {
		const disp = Object.fromEntries(ids.map((id) => [id, { x: 0, y: 0 }]));
		for (let i = 0; i < ids.length; i += 1) {
			for (let j = i + 1; j < ids.length; j += 1) {
				const a = positions[ids[i]];
				const b = positions[ids[j]];
				let dx = a.x - b.x || 0.5;
				let dy = a.y - b.y || 0.5;
				const dist = Math.max(1, Math.hypot(dx, dy));
				const force = 640 / (dist * dist);
				dx = (dx / dist) * force;
				dy = (dy / dist) * force;
				disp[ids[i]].x += dx;
				disp[ids[i]].y += dy;
				disp[ids[j]].x -= dx;
				disp[ids[j]].y -= dy;
			}
		}
		for (const edge of edges) {
			const a = positions[edge.fromId];
			const b = positions[edge.toId];
			if (!a || !b) continue;
			const dx = b.x - a.x;
			const dy = b.y - a.y;
			const dist = Math.max(1, Math.hypot(dx, dy));
			const force = (dist - 180) * 0.015;
			disp[edge.fromId].x += (dx / dist) * force;
			disp[edge.fromId].y += (dy / dist) * force;
			disp[edge.toId].x -= (dx / dist) * force;
			disp[edge.toId].y -= (dy / dist) * force;
		}
		for (const id of ids) {
			positions[id].x += Math.max(-14, Math.min(14, disp[id].x));
			positions[id].y += Math.max(-14, Math.min(14, disp[id].y));
		}
	}
	return positions;
}
