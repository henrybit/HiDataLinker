import type {
	Cardinality,
	CatalogObject,
	InferredComment,
	InferredRelation,
	RelationshipEdge,
	RelationshipGraph,
	SchemaCatalog
} from './types';

export function assembleGraph(
	catalog: SchemaCatalog,
	comments: InferredComment[],
	relations: InferredRelation[]
): RelationshipGraph {
	const nodes = catalog.objects.map((object) => ({
		...object,
		columns: object.columns.map((column) => ({ ...column }))
	}));
	const byId = new Map(nodes.map((node) => [node.id, node]));
	for (const comment of comments) {
		const text = comment.comment.trim();
		if (!text) continue;
		const node = byId.get(comment.objectId);
		if (!node || node.external) continue;
		if (comment.columnName) {
			const column = node.columns.find((item) => item.name === comment.columnName);
			if (!column || column.comment.trim() || column.inferredComment) continue;
			column.inferredComment = text.slice(0, 200);
			column.inferenceConfidence = comment.confidence;
		} else if (!node.comment.trim() && !node.inferredComment) {
			node.inferredComment = text.slice(0, 200);
			node.inferenceConfidence = comment.confidence;
		}
	}

	const known = [...buildPhysicalEdges(catalog), ...buildViewEdges(catalog)];
	const inferred = relations.flatMap((relation) => {
		if (!byId.has(relation.fromId) || !byId.has(relation.toId)) return [];
		if (relation.fromId === relation.toId) return [];
		const fromColumns = knownColumns(byId.get(relation.fromId), relation.fromColumns);
		const toColumns = knownColumns(byId.get(relation.toId), relation.toColumns);
		const duplicate = known.some(
			(edge) =>
				edge.fromId === relation.fromId &&
				edge.toId === relation.toId &&
				sameSet(edge.fromColumns, fromColumns) &&
				sameSet(edge.toColumns, toColumns)
		);
		if (duplicate) return [];
		const edge: RelationshipEdge = {
			id: `inferred:${relation.fromId}:${relation.toId}:${fromColumns.join(',')}:${toColumns.join(',')}:${relation.cardinality}`,
			fromId: relation.fromId,
			toId: relation.toId,
			fromColumns,
			toColumns,
			origin: 'inferred',
			strength: relation.strength,
			cardinality: relation.cardinality,
			reasonCode: 'inferred',
			reason: relation.reason.trim().slice(0, 500),
			confidence: relation.confidence
		};
		return [edge];
	});

	const seen = new Set<string>();
	const edges = [...known, ...inferred].filter((edge) => {
		if (seen.has(edge.id)) return false;
		seen.add(edge.id);
		return true;
	});
	return { nodes, edges, warnings: catalog.warnings };
}

export function buildPhysicalEdges(catalog: SchemaCatalog): RelationshipEdge[] {
	const byId = new Map(catalog.objects.map((object) => [object.id, object]));
	const edges: RelationshipEdge[] = [];
	for (const constraint of catalog.foreignKeys) {
		const from = byId.get(constraint.fromId);
		if (!from) continue;
		edges.push({
			id: `fk:${constraint.id}`,
			fromId: constraint.fromId,
			toId: constraint.toId,
			fromColumns: constraint.pairs.map((pair) => pair.fromColumn),
			toColumns: constraint.pairs.map((pair) => pair.toColumn),
			origin: 'physical',
			strength: 'strong',
			cardinality: fkCardinality(
				from,
				constraint.pairs.map((pair) => pair.fromColumn)
			),
			reasonCode: 'foreign-key',
			reason: constraint.name
		});
	}
	edges.push(...junctionEdges(catalog, edges));
	return edges;
}

export function buildViewEdges(catalog: SchemaCatalog): RelationshipEdge[] {
	const tables = catalog.objects.filter((object) => object.kind === 'table');
	const edges: RelationshipEdge[] = [];
	for (const view of catalog.objects) {
		if (view.kind !== 'view' || !view.definition.trim()) continue;
		const sql = normalizeSql(view.definition);
		for (const table of tables) {
			if (table.id === view.id || table.connectionId !== view.connectionId) continue;
			if (!viewReferences(sql, view, table)) continue;
			edges.push({
				id: `view:${view.id}:${table.id}`,
				fromId: view.id,
				toId: table.id,
				fromColumns: [],
				toColumns: [],
				origin: 'view',
				strength: 'strong',
				cardinality: 'many_to_one',
				reasonCode: 'view-reference',
				reason: ''
			});
		}
	}
	return edges;
}

function junctionEdges(
	catalog: SchemaCatalog,
	foreignKeys: RelationshipEdge[]
): RelationshipEdge[] {
	const byFrom = new Map<string, RelationshipEdge[]>();
	for (const edge of foreignKeys) {
		if (edge.origin !== 'physical' || edge.cardinality !== 'many_to_one') continue;
		const list = byFrom.get(edge.fromId) ?? [];
		list.push(edge);
		byFrom.set(edge.fromId, list);
	}
	const objects = new Map(catalog.objects.map((object) => [object.id, object]));
	const edges: RelationshipEdge[] = [];
	for (const [fromId, links] of byFrom) {
		if (links.length !== 2) continue;
		const [left, right] = links;
		if (left.toId === right.toId) continue;
		const junction = objects.get(fromId);
		if (!junction) continue;
		const pk = junction.columns
			.filter((column) => column.key === 'PRI')
			.map((column) => column.name);
		const fkColumns = [...left.fromColumns, ...right.fromColumns];
		if (pk.length === 0 || !sameSet(pk, fkColumns)) continue;
		const [from, to] = [left.toId, right.toId].sort();
		const fromEdge = from === left.toId ? left : right;
		const toEdge = fromEdge === left ? right : left;
		edges.push({
			id: `m2m:${fromId}:${from}:${to}`,
			fromId: from,
			toId: to,
			fromColumns: fromEdge.toColumns,
			toColumns: toEdge.toColumns,
			origin: 'physical',
			strength: 'strong',
			cardinality: 'many_to_many',
			reasonCode: 'junction',
			reason: '',
			viaId: fromId
		});
	}
	return edges;
}

function fkCardinality(from: CatalogObject, fromColumns: string[]): Cardinality {
	const pk = from.columns.filter((column) => column.key === 'PRI').map((column) => column.name);
	const unique =
		pk.length > 0 &&
		pk.length === fromColumns.length &&
		fromColumns.every((column) => pk.includes(column));
	return unique ? 'one_to_one' : 'many_to_one';
}

function viewReferences(sql: string, view: CatalogObject, table: CatalogObject): boolean {
	const qualified = `${table.schema}.${table.name}`.toLowerCase();
	if (includesIdent(sql, qualified)) return true;
	return (
		table.schema.toLowerCase() === view.schema.toLowerCase() &&
		includesIdent(sql, table.name.toLowerCase())
	);
}

function includesIdent(sql: string, ident: string): boolean {
	const escaped = ident.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
	return new RegExp(`(^|[^a-z0-9_])${escaped}([^a-z0-9_]|$)`, 'i').test(sql);
}

function normalizeSql(sql: string): string {
	return sql
		.toLowerCase()
		.replaceAll('[', '')
		.replaceAll(']', '')
		.replaceAll('"', '')
		.replaceAll('`', '');
}

function knownColumns(object: CatalogObject | undefined, columns: string[]): string[] {
	if (!object || object.columns.length === 0) return columns;
	const names = new Map(object.columns.map((column) => [column.name.toLowerCase(), column.name]));
	return columns.flatMap((column) => {
		const match = names.get(column.toLowerCase());
		return match ? [match] : [];
	});
}

function sameSet(left: string[], right: string[]): boolean {
	if (left.length !== right.length) return false;
	const values = new Set(left.map((item) => item.toLowerCase()));
	return right.every((item) => values.has(item.toLowerCase()));
}
