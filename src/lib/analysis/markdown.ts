import { t } from '$lib/i18n/i18n.svelte';
import type {
	AnalysisWarning,
	Cardinality,
	CatalogObject,
	Confidence,
	RelationshipEdge,
	RelationshipGraph
} from './types';

export function renderRelationshipMarkdown(
	graph: RelationshipGraph,
	generatedAt = new Date().toISOString()
): string {
	const nodes = [...graph.nodes].sort(compareObjects);
	const edges = [...graph.edges].sort((left, right) =>
		`${nodeTitle(graph.nodes, left.fromId)} ${nodeTitle(graph.nodes, left.toId)}`.localeCompare(
			`${nodeTitle(graph.nodes, right.fromId)} ${nodeTitle(graph.nodes, right.toId)}`
		)
	);
	const physical = edges.filter((edge) => edge.origin === 'physical').length;
	const views = edges.filter((edge) => edge.origin === 'view').length;
	const inferred = edges.filter((edge) => edge.origin === 'inferred').length;
	const strong = edges.filter((edge) => edge.strength === 'strong').length;
	const weak = edges.filter((edge) => edge.strength === 'weak').length;
	const lines = [
		`# ${t('analysis.title')}`,
		'',
		generatedAt,
		'',
		`## ${t('analysis.md.overview')}`,
		'',
		`- ${t('analysis.summary', { objects: nodes.length, edges: edges.length })}`,
		`- ${t('analysis.md.breakdown', { physical, views, inferred, strong, weak })}`,
		''
	];
	if (graph.warnings.length > 0) {
		lines.push(`## ${t('analysis.md.warnings')}`, '');
		for (const warning of graph.warnings) lines.push(`- ${warningText(warning)}`);
		lines.push('');
	}
	lines.push(`## ${t('analysis.md.diagram')}`, '', ...mermaidSection(nodes, edges), '');
	lines.push(`## ${t('analysis.md.objects')}`, '');
	let group = '';
	for (const node of nodes) {
		const nextGroup = `${node.connectionName} / ${node.schema}`;
		if (nextGroup !== group) {
			group = nextGroup;
			lines.push(`### ${cell(group)}`, '');
		}
		lines.push(...objectSection(node), '');
	}
	lines.push(`## ${t('analysis.md.relationships')}`, '');
	if (edges.length === 0) {
		lines.push(t('analysis.md.noRelationships'), '');
	} else {
		lines.push(
			`| ${t('analysis.from')} | ${t('analysis.to')} | ${t('analysis.kind')} | ${t('analysis.cardinality')} | ${t('analysis.reason')} |`,
			'| --- | --- | --- | --- | --- |'
		);
		for (const edge of edges) {
			lines.push(
				`| ${endpoint(graph.nodes, edge.fromId, edge.fromColumns)} | ${endpoint(graph.nodes, edge.toId, edge.toColumns)} | ${cell(`${originLabel(edge)} · ${strengthLabel(edge)}`)} | ${cell(cardinalityText(edge.cardinality))} | ${cell(edgeReason(edge, graph.nodes))} |`
			);
		}
		lines.push('');
	}
	return (
		lines
			.join('\n')
			.replace(/\n{3,}/g, '\n\n')
			.trim() + '\n'
	);
}

function mermaidSection(nodes: CatalogObject[], edges: RelationshipEdge[]): string[] {
	if (nodes.length === 0) return [t('analysis.noObjects')];
	const ids = new Map(nodes.map((node, index) => [node.id, `n${index + 1}`]));
	const lines = ['```mermaid', 'flowchart LR'];
	for (const node of nodes) {
		const id = ids.get(node.id);
		if (id) lines.push(mermaidNode(node, id, nodes));
	}
	for (const edge of edges) {
		const from = ids.get(edge.fromId);
		const to = ids.get(edge.toId);
		if (!from || !to) continue;
		const arrow = edge.strength === 'weak' ? '-.->' : '-->';
		lines.push(`  ${from} ${arrow}|"${mermaidLabel(edgeLabel(edge))}"| ${to}`);
	}
	lines.push('```');
	return lines;
}

function mermaidNode(node: CatalogObject, id: string, nodes: CatalogObject[]): string {
	const label = mermaidLabel(diagramName(node, nodes));
	if (node.kind === 'view') return `  ${id}(["${label}"])`;
	if (node.external) return `  ${id}{{"${label}"}}`;
	return `  ${id}["${label}"]`;
}

function diagramName(node: CatalogObject, nodes: CatalogObject[]): string {
	const base = `${node.schema}.${node.name}`;
	const duplicated = nodes.some(
		(other) => other.id !== node.id && other.schema === node.schema && other.name === node.name
	);
	return duplicated ? `${node.connectionName} / ${base}` : base;
}

function edgeLabel(edge: RelationshipEdge): string {
	const from = edge.fromColumns.filter(Boolean).join('+');
	const to = edge.toColumns.filter(Boolean).join('+');
	const fields = from || to ? `${from} → ${to}` : '';
	return [fields, cardinalityText(edge.cardinality)].filter(Boolean).join(' · ');
}

function mermaidLabel(value: string): string {
	return value
		.replace(/["#<>|]/g, (char) => {
			if (char === '"') return "'";
			if (char === '#') return '';
			if (char === '<') return '(';
			if (char === '>') return ')';
			return '/';
		})
		.replace(/\s+/g, ' ')
		.trim();
}

function objectSection(node: CatalogObject): string[] {
	const kind = node.kind === 'view' ? t('analysis.node.view') : t('analysis.node.table');
	const marks = [kind];
	if (node.external) marks.push(t('analysis.external'));
	const comment = node.comment.trim() || node.inferredComment?.trim() || '';
	const lines = [`#### ${cell(node.name)}`, '', marks.join(' · ')];
	if (comment) {
		lines.push(
			'',
			node.inferredComment && !node.comment.trim()
				? `${comment} (${t('analysis.comment.inferred')}${node.inferenceConfidence ? `, ${confidenceLabel(node.inferenceConfidence)}` : ''})`
				: comment
		);
	}
	if (node.columns.length === 0) return lines;
	lines.push(
		'',
		`| ${t('analysis.md.column')} | ${t('analysis.md.type')} | ${t('analysis.md.key')} | ${t('analysis.md.note')} |`,
		'| --- | --- | --- | --- |'
	);
	for (const column of node.columns) {
		const note =
			column.comment.trim() || column.inferredComment?.trim() || t('analysis.comment.none');
		const inferred =
			column.inferredComment && !column.comment.trim()
				? ` (${t('analysis.comment.inferred')}${column.inferenceConfidence ? `, ${confidenceLabel(column.inferenceConfidence)}` : ''})`
				: '';
		lines.push(
			`| ${cell(column.name)} | ${cell(column.dataType)} | ${cell(column.key)} | ${cell(note + inferred)} |`
		);
	}
	return lines;
}

function endpoint(nodes: CatalogObject[], id: string, columns: string[]): string {
	const name = nodeTitle(nodes, id);
	const fields = columns.filter(Boolean).join(', ');
	return cell(fields ? `${name} (${fields})` : name);
}

function nodeTitle(nodes: CatalogObject[], id: string): string {
	const node = nodes.find((item) => item.id === id);
	return node ? `${node.schema}.${node.name}` : id;
}

function warningText(warning: AnalysisWarning): string {
	if (warning.code === 'truncated') {
		return t('analysis.truncated', { name: `${warning.connectionName}.${warning.schema}` });
	}
	return t('analysis.noObjects');
}

function edgeReason(edge: RelationshipEdge, nodes: CatalogObject[]): string {
	if (edge.reasonCode === 'foreign-key') return t('analysis.reason.foreignKey');
	if (edge.reasonCode === 'view-reference') return t('analysis.reason.view');
	if (edge.reasonCode === 'junction') {
		const via = nodes.find((node) => node.id === edge.viaId);
		return t('analysis.reason.junction', { name: via?.name ?? '' });
	}
	return edge.reason;
}

function originLabel(edge: RelationshipEdge): string {
	if (edge.origin === 'view') return t('analysis.origin.view');
	if (edge.origin === 'inferred') return t('analysis.origin.inferred');
	return t('analysis.origin.physical');
}

function strengthLabel(edge: RelationshipEdge): string {
	return edge.strength === 'weak' ? t('analysis.weak') : t('analysis.strong');
}

function cardinalityText(value: Cardinality): string {
	if (value === 'one_to_one') return t('analysis.cardinality.one_to_one');
	if (value === 'one_to_many') return t('analysis.cardinality.one_to_many');
	if (value === 'many_to_one') return t('analysis.cardinality.many_to_one');
	return t('analysis.cardinality.many_to_many');
}

function confidenceLabel(value: Confidence): string {
	if (value === 'high') return t('analysis.confidence.high');
	if (value === 'medium') return t('analysis.confidence.medium');
	return t('analysis.confidence.low');
}

function compareObjects(left: CatalogObject, right: CatalogObject): number {
	return `${left.connectionName}/${left.schema}/${left.name}`.localeCompare(
		`${right.connectionName}/${right.schema}/${right.name}`
	);
}

function cell(value: string): string {
	return value.replace(/\|/g, '\\|').replace(/\s+/g, ' ').trim();
}
