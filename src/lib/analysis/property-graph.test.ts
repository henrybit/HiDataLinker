import { describe, expect, it } from 'vitest';
import { assembleGraph } from './physical';
import { buildPropertyGraph, propertyGraphConnections } from './property-graph';
import { objectId, type CatalogObject, type GraphFilters, type SchemaCatalog } from './types';

const all: GraphFilters = { physical: true, inferred: true, strong: true, weak: true };

function table(name: string, columns: Array<[string, string, boolean?]>): CatalogObject {
	return {
		id: objectId('pg', 'shop', name),
		connectionId: 'pg',
		connectionName: 'pg19',
		engine: 'postgres',
		schema: 'shop',
		name,
		kind: 'table',
		comment: '',
		definition: '',
		external: false,
		columns: columns.map(([column, dataType, pk], index) => ({
			name: column,
			dataType,
			key: pk ? 'PRI' : '',
			comment: '',
			ordinal: index + 1
		}))
	};
}

function fk(from: string, to: string, pairs: Array<[string, string]>) {
	return {
		id: `${from}_${to}_fk`,
		name: `${from}_${to}_fk`,
		connectionId: 'pg',
		fromId: objectId('pg', 'shop', from),
		toId: objectId('pg', 'shop', to),
		pairs: pairs.map(([fromColumn, toColumn]) => ({ fromColumn, toColumn }))
	};
}

const fixtureCatalog: SchemaCatalog = {
	objects: [
		table('customers', [
			['id', 'integer', true],
			['name', 'text']
		]),
		table('orders', [
			['id', 'integer', true],
			['customer_id', 'integer'],
			['name', 'character varying(20)']
		]),
		table('products', [
			['id', 'integer', true],
			['title', 'text']
		]),
		table('order_items', [
			['order_id', 'integer', true],
			['product_id', 'integer', true],
			['qty', 'integer']
		]),
		table('emp', [
			['id', 'integer', true],
			['manager_id', 'integer']
		]),
		table('shipments', [
			['id', 'integer', true],
			['order_no', 'integer']
		]),
		table('audit_log', [['order_ref', 'integer']])
	],
	foreignKeys: [
		fk('orders', 'customers', [['customer_id', 'id']]),
		fk('order_items', 'orders', [['order_id', 'id']]),
		fk('order_items', 'products', [['product_id', 'id']]),
		fk('emp', 'emp', [['manager_id', 'id']])
	],
	warnings: []
};

const fixtureGraph = assembleGraph(
	fixtureCatalog,
	[],
	[
		{
			fromId: objectId('pg', 'shop', 'shipments'),
			toId: objectId('pg', 'shop', 'orders'),
			fromColumns: ['order_no'],
			toColumns: ['id'],
			strength: 'weak',
			cardinality: 'many_to_one',
			reason: 'order_no matches orders.id',
			confidence: 'medium'
		},
		{
			fromId: objectId('pg', 'shop', 'audit_log'),
			toId: objectId('pg', 'shop', 'orders'),
			fromColumns: ['order_ref'],
			toColumns: ['id'],
			strength: 'weak',
			cardinality: 'many_to_one',
			reason: 'order_ref',
			confidence: 'low'
		},
		{
			fromId: objectId('pg', 'shop', 'customers'),
			toId: objectId('pg', 'shop', 'products'),
			fromColumns: ['id'],
			toColumns: ['id'],
			strength: 'weak',
			cardinality: 'many_to_many',
			reason: 'customers buy products',
			confidence: 'low'
		}
	]
);

describe('buildPropertyGraph', () => {
	const plan = buildPropertyGraph(fixtureGraph, 'pg', all)!;

	it('turns tables with a primary key into vertices and skips the rest', () => {
		expect(plan.graphName).toBe('shop_graph');
		expect(plan.vertexCount).toBe(5);
		expect(plan.skippedTables).toEqual(['shop.audit_log']);
		expect(plan.createSql.split('EDGE TABLES')[0]).not.toContain('order_items');
	});

	it('maps foreign keys, junction tables and inferred relations to edges', () => {
		expect(plan.edgeCount).toBe(4);
		expect(plan.inferredCount).toBe(1);
		expect(plan.skippedEdges).toBe(2);
		expect(plan.createSql).toContain(
			'"shop"."orders" AS "orders_customers" KEY ("id")\n' +
				'            SOURCE KEY ("id") REFERENCES "orders" ("id")\n' +
				'            DESTINATION KEY ("customer_id") REFERENCES "customers" ("id")\n' +
				'            NO PROPERTIES'
		);
		expect(plan.createSql).toContain(
			'"shop"."order_items" AS "order_items" KEY ("order_id", "product_id")\n' +
				'            SOURCE KEY ("order_id") REFERENCES "orders" ("id")\n' +
				'            DESTINATION KEY ("product_id") REFERENCES "products" ("id")'
		);
		expect(plan.createSql).toContain('REFERENCES "emp" ("id")');
		expect(plan.createSql).toContain('-- inferred (medium)');
	});

	it('renames properties whose type differs between tables', () => {
		expect(plan.createSql).toContain(
			'"shop"."customers" AS "customers" KEY ("id") PROPERTIES ("id", "name" AS "customers_name")'
		);
		expect(plan.createSql).toContain('"name" AS "orders_name"');
		expect(plan.createSql).toContain('"shop"."products" AS "products" KEY ("id"),');
	});

	it('writes a two-hop GRAPH_TABLE example', () => {
		expect(plan.exampleSql).toMatch(
			/MATCH \(a IS "\w+"\)-\[IS "\w+"\]->\(b IS "\w+"\)-\[IS "\w+"\]->\(c IS "\w+"\)/
		);
	});

	it('respects the graph filters', () => {
		const physical = buildPropertyGraph(fixtureGraph, 'pg', { ...all, inferred: false })!;
		expect(physical.inferredCount).toBe(0);
		expect(physical.edgeCount).toBe(3);
	});
});

describe('propertyGraphConnections', () => {
	it('lists PostgreSQL connections with tables', () => {
		expect(propertyGraphConnections(fixtureGraph)).toEqual(['pg']);
		const mysql = {
			...fixtureGraph,
			nodes: fixtureGraph.nodes.map((node) => ({ ...node, engine: 'mysql' }))
		};
		expect(propertyGraphConnections(mysql)).toEqual([]);
		expect(buildPropertyGraph(mysql, 'pg', all)).toBeNull();
	});
});
