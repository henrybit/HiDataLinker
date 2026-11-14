import { isPostgres, qualifyIdent, quoteIdent } from '$lib/engine';
import { edgeVisible } from './layout';
import type { CatalogObject, GraphFilters, RelationshipEdge, RelationshipGraph } from './types';

/** `CREATE PROPERTY GRAPH` (PostgreSQL 19+) derived from a relationship graph. */
export interface PropertyGraphPlan {
	connectionId: string;
	schema: string;
	graphName: string;
	createSql: string;
	exampleSql: string;
	vertexCount: number;
	edgeCount: number;
	inferredCount: number;
	/** Tables left out because they have no primary key to use as element key. */
	skippedTables: string[];
	/** Relations that cannot be expressed as edges (no key columns, many-to-many without junction). */
	skippedEdges: number;
}

interface Vertex {
	node: CatalogObject;
	alias: string;
	key: string[];
}

interface Edge {
	table: CatalogObject;
	alias: string;
	key: string[];
	source: { vertex: Vertex; columns: string[]; references: string[] };
	destination: { vertex: Vertex; columns: string[]; references: string[] };
	/** Junction tables carry their columns as edge properties; FK edges reuse the vertex row. */
	properties: boolean;
	note: string;
	inferred: boolean;
}

const q = (name: string) => quoteIdent('postgres', name);
const list = (names: string[]) => names.map(q).join(', ');

function primaryKey(node: CatalogObject): string[] {
	return [...node.columns]
		.filter((column) => column.key === 'PRI')
		.sort((a, b) => a.ordinal - b.ordinal)
		.map((column) => column.name);
}

function uniqueName(base: string, taken: Set<string>): string {
	let name = base;
	for (let index = 2; taken.has(name.toLowerCase()); index += 1) name = `${base}_${index}`;
	taken.add(name.toLowerCase());
	return name;
}

/** PostgreSQL connections in the graph, the one with the most tables first. */
export function propertyGraphConnections(graph: RelationshipGraph): string[] {
	const counts = new Map<string, number>();
	for (const node of graph.nodes) {
		if (node.kind !== 'table' || !isPostgres(node.engine)) continue;
		counts.set(node.connectionId, (counts.get(node.connectionId) ?? 0) + 1);
	}
	return [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([id]) => id);
}

export function buildPropertyGraph(
	graph: RelationshipGraph,
	connectionId: string,
	filters: GraphFilters
): PropertyGraphPlan | null {
	const tables = graph.nodes.filter(
		(node) => node.connectionId === connectionId && node.kind === 'table' && isPostgres(node.engine)
	);
	if (tables.length === 0) return null;
	const byId = new Map(tables.map((node) => [node.id, node]));
	const candidates = graph.edges.filter(
		(edge) =>
			(edge.origin === 'physical' || edge.origin === 'inferred') &&
			edgeVisible(edge, filters) &&
			byId.has(edge.fromId) &&
			byId.has(edge.toId)
	);

	const junctionLinks = new Map<
		string,
		{ junction: CatalogObject; left: RelationshipEdge; right: RelationshipEdge }
	>();
	for (const edge of candidates) {
		const junction = edge.viaId ? byId.get(edge.viaId) : undefined;
		if (!junction || primaryKey(junction).length === 0) continue;
		const link = (toId: string) =>
			graph.edges.find(
				(item) => item.origin === 'physical' && item.fromId === junction.id && item.toId === toId
			);
		const left = link(edge.fromId);
		const right = link(edge.toId);
		if (left && right) junctionLinks.set(edge.id, { junction, left, right });
	}
	const junctionIds = new Set([...junctionLinks.values()].map((link) => link.junction.id));

	const aliases = new Set<string>();
	const nameCounts = new Map<string, number>();
	for (const node of tables) {
		nameCounts.set(node.name.toLowerCase(), (nameCounts.get(node.name.toLowerCase()) ?? 0) + 1);
	}
	const vertices = new Map<string, Vertex>();
	const skippedTables: string[] = [];
	for (const node of tables) {
		if (junctionIds.has(node.id)) continue;
		const key = primaryKey(node);
		if (key.length === 0) {
			skippedTables.push(`${node.schema}.${node.name}`);
			continue;
		}
		const base =
			(nameCounts.get(node.name.toLowerCase()) ?? 0) > 1
				? `${node.schema}_${node.name}`
				: node.name;
		vertices.set(node.id, { node, key, alias: uniqueName(base, aliases) });
	}

	const edges: Edge[] = [];
	let skippedEdges = 0;
	const usedLinks = new Set(
		[...junctionLinks.values()].flatMap(({ left, right }) => [left.id, right.id])
	);
	for (const edge of candidates) {
		if (usedLinks.has(edge.id)) continue;
		const link = junctionLinks.get(edge.id);
		if (link) {
			const { junction, left, right } = link;
			const source = vertices.get(left.toId);
			const destination = vertices.get(right.toId);
			if (!source || !destination) {
				skippedEdges += 1;
				continue;
			}
			edges.push({
				table: junction,
				alias: uniqueName(junction.name, aliases),
				key: primaryKey(junction),
				source: { vertex: source, columns: left.fromColumns, references: left.toColumns },
				destination: {
					vertex: destination,
					columns: right.fromColumns,
					references: right.toColumns
				},
				properties: true,
				note: `junction ${junction.name}`,
				inferred: false
			});
			continue;
		}
		if (edge.cardinality === 'many_to_many') {
			skippedEdges += 1;
			continue;
		}
		const forward = edge.cardinality !== 'one_to_many';
		const child = vertices.get(forward ? edge.fromId : edge.toId);
		const parent = vertices.get(forward ? edge.toId : edge.fromId);
		const childColumns = forward ? edge.fromColumns : edge.toColumns;
		const parentColumns = forward ? edge.toColumns : edge.fromColumns;
		if (
			!child ||
			!parent ||
			childColumns.length === 0 ||
			childColumns.length !== parentColumns.length
		) {
			skippedEdges += 1;
			continue;
		}
		const inferred = edge.origin === 'inferred';
		edges.push({
			table: child.node,
			alias: uniqueName(`${child.node.name}_${parent.node.name}`, aliases),
			key: child.key,
			source: { vertex: child, columns: child.key, references: child.key },
			destination: { vertex: parent, columns: childColumns, references: parentColumns },
			properties: false,
			note: inferred
				? `inferred${edge.confidence ? ` (${edge.confidence})` : ''}`
				: `foreign key ${edge.reason}`.trim(),
			inferred
		});
	}

	const properties = propertyLists([
		...[...vertices.values()].map((vertex) => ({ node: vertex.node, alias: vertex.alias })),
		...edges
			.filter((edge) => edge.properties)
			.map((edge) => ({ node: edge.table, alias: edge.alias }))
	]);

	const schema =
		mostCommonSchema([...vertices.values()].map((vertex) => vertex.node.schema)) ??
		tables[0].schema;
	const graphName = `${schema}_graph`.toLowerCase().replace(/[^a-z0-9_]/g, '_');
	const vertexLines = [...vertices.values()].map(
		(vertex) =>
			`        ${qualifyIdent('postgres', vertex.node.schema, vertex.node.name)} AS ${q(vertex.alias)} KEY (${list(vertex.key)})${properties.get(vertex.alias) ?? ''}`
	);
	const edgeLines = edges.map((edge) =>
		[
			`        -- ${edge.note.replace(/\s+/g, ' ')}`,
			`        ${qualifyIdent('postgres', edge.table.schema, edge.table.name)} AS ${q(edge.alias)} KEY (${list(edge.key)})`,
			`            SOURCE KEY (${list(edge.source.columns)}) REFERENCES ${q(edge.source.vertex.alias)} (${list(edge.source.references)})`,
			`            DESTINATION KEY (${list(edge.destination.columns)}) REFERENCES ${q(edge.destination.vertex.alias)} (${list(edge.destination.references)})`,
			edge.properties ? (properties.get(edge.alias) ?? '').trimStart() || null : 'NO PROPERTIES'
		]
			.filter((line): line is string => line !== null)
			.map((line, index) => (index >= 4 ? `            ${line}` : line))
			.join('\n')
	);
	const qualifiedGraph = qualifyIdent('postgres', schema, graphName);
	let createSql = `CREATE PROPERTY GRAPH ${qualifiedGraph}\n    VERTEX TABLES (\n${vertexLines.join(',\n')}\n    )`;
	if (edgeLines.length) createSql += `\n    EDGE TABLES (\n${edgeLines.join(',\n')}\n    )`;

	return {
		connectionId,
		schema,
		graphName,
		createSql,
		exampleSql: exampleQuery(qualifiedGraph, [...vertices.values()], edges),
		vertexCount: vertices.size,
		edgeCount: edges.length,
		inferredCount: edges.filter((edge) => edge.inferred).length,
		skippedTables,
		skippedEdges
	};
}

/**
 * A property name must have one data type across the whole graph, so columns
 * whose name clashes with a differently typed column elsewhere get prefixed.
 */
function propertyLists(
	elements: Array<{ node: CatalogObject; alias: string }>
): Map<string, string> {
	const types = new Map<string, Set<string>>();
	for (const { node } of elements) {
		for (const column of node.columns) {
			const name = column.name.toLowerCase();
			types.set(name, (types.get(name) ?? new Set()).add(column.dataType.toLowerCase()));
		}
	}
	const out = new Map<string, string>();
	for (const { node, alias } of elements) {
		const clashes = node.columns.some(
			(column) => (types.get(column.name.toLowerCase())?.size ?? 0) > 1
		);
		if (!clashes) continue;
		const items = [...node.columns]
			.sort((a, b) => a.ordinal - b.ordinal)
			.map((column) =>
				(types.get(column.name.toLowerCase())?.size ?? 0) > 1
					? `${q(column.name)} AS ${q(`${alias}_${column.name}`)}`
					: q(column.name)
			);
		out.set(alias, ` PROPERTIES (${items.join(', ')})`);
	}
	return out;
}

function mostCommonSchema(schemas: string[]): string | undefined {
	const counts = new Map<string, number>();
	for (const schema of schemas) counts.set(schema, (counts.get(schema) ?? 0) + 1);
	return [...counts.entries()].sort((a, b) => b[1] - a[1])[0]?.[0];
}

/** Prefers a two-hop path so the example shows what graph queries add over joins. */
function exampleQuery(graphName: string, vertices: Vertex[], edges: Edge[]): string {
	const keyColumns = (variable: string, vertex: Vertex) =>
		vertex.key.map((column) => `${variable}.${q(column)} AS ${q(`${variable}_${column}`)}`);
	const node = (variable: string, vertex: Vertex) => `(${variable} IS ${q(vertex.alias)})`;
	const hop = (edge: Edge) => `-[IS ${q(edge.alias)}]->`;

	let match: string;
	let columns: string[];
	const path = edges
		.flatMap((first) =>
			edges
				.filter(
					(second) =>
						second !== first &&
						second.source.vertex === first.destination.vertex &&
						second.destination.vertex !== first.source.vertex
				)
				.map((second) => [first, second] as const)
		)
		.at(0);
	if (path) {
		const [first, second] = path;
		match = `${node('a', first.source.vertex)}${hop(first)}${node('b', first.destination.vertex)}${hop(second)}${node('c', second.destination.vertex)}`;
		columns = [
			...keyColumns('a', first.source.vertex),
			...keyColumns('b', first.destination.vertex),
			...keyColumns('c', second.destination.vertex)
		];
	} else if (edges[0]) {
		const edge = edges[0];
		match = `${node('a', edge.source.vertex)}${hop(edge)}${node('b', edge.destination.vertex)}`;
		columns = [...keyColumns('a', edge.source.vertex), ...keyColumns('b', edge.destination.vertex)];
	} else {
		const vertex = vertices[0];
		match = node('a', vertex);
		columns = keyColumns('a', vertex);
	}
	return `SELECT *\nFROM GRAPH_TABLE (${graphName}\n    MATCH ${match}\n    COLUMNS (${columns.join(', ')})\n)\nLIMIT 20`;
}
