import { describe, expect, it } from 'vitest';
import type { QueryResult } from '$lib/api/types';
import { catalogFromDumps } from './collect';
import { assembleGraph, buildPhysicalEdges, buildViewEdges } from './physical';
import { objectId, type CatalogObject, type SchemaCatalog, type SchemaScope } from './types';

const scope: SchemaScope = {
	connectionId: 'c1',
	connectionName: 'App',
	engine: 'mysql',
	schema: 'shop'
};

function query(columns: string[], rows: Array<Array<string | null>>): QueryResult {
	return {
		columns: columns.map((name) => ({ name, typeName: 'text' })),
		rows,
		affectedRows: rows.length,
		durationMs: 1,
		truncated: false,
		statementKind: 'query'
	};
}

function column(name: string, key = '', comment = ''): CatalogObject['columns'][number] {
	return { name, dataType: 'int', key, comment, ordinal: 1 };
}

describe('catalogFromDumps', () => {
	it('groups composite foreign keys and stubs a table outside the scope', () => {
		const catalog = catalogFromDumps([
			{
				scope,
				objects: query(
					['object_name', 'object_kind', 'comment'],
					[
						['orders', 'table', ''],
						['open_orders', 'view', '']
					]
				),
				columns: query(
					[
						'object_name',
						'column_name',
						'column_type',
						'column_key',
						'comment',
						'ordinal_position'
					],
					[
						['orders', 'id', 'int', 'PRI', '', '1'],
						['orders', 'user_id', 'int', '', '', '2']
					]
				),
				foreignKeys: query(
					[
						'constraint_name',
						'from_table',
						'from_column',
						'to_schema',
						'to_table',
						'to_column',
						'ordinal_position'
					],
					[
						['orders_user_fk', 'orders', 'user_id', 'accounts', 'users', 'id', '1'],
						['orders_user_fk', 'orders', 'tenant_id', 'accounts', 'users', 'tenant_id', '2']
					]
				),
				views: query(['object_name', 'definition'], [['open_orders', 'select * from orders']])
			}
		]);

		expect(catalog.foreignKeys).toHaveLength(1);
		expect(catalog.foreignKeys[0]?.pairs).toEqual([
			{ fromColumn: 'user_id', toColumn: 'id' },
			{ fromColumn: 'tenant_id', toColumn: 'tenant_id' }
		]);
		const stub = catalog.objects.find((object) => object.name === 'users');
		expect(stub?.external).toBe(true);
		expect(stub?.schema).toBe('accounts');
		expect(catalog.objects.find((object) => object.name === 'open_orders')?.definition).toContain(
			'orders'
		);
	});
});

describe('physical relationships', () => {
	it('marks a primary-key foreign key as one-to-one and detects a junction table', () => {
		const users = table('users', [column('id', 'PRI')]);
		const roles = table('roles', [column('id', 'PRI')]);
		const profiles = table('profiles', [column('user_id', 'PRI')]);
		const userRoles = table('user_roles', [column('user_id', 'PRI'), column('role_id', 'PRI')]);
		const catalog: SchemaCatalog = {
			objects: [users, roles, profiles, userRoles],
			foreignKeys: [
				fk('profiles_user', profiles, users, [['user_id', 'id']]),
				fk('user_roles_user', userRoles, users, [['user_id', 'id']]),
				fk('user_roles_role', userRoles, roles, [['role_id', 'id']])
			],
			warnings: []
		};
		const edges = buildPhysicalEdges(catalog);
		expect(edges.find((edge) => edge.fromId === profiles.id)?.cardinality).toBe('one_to_one');
		expect(
			edges.find((edge) => edge.fromId === userRoles.id && edge.toId === users.id)?.cardinality
		).toBe('many_to_one');
		const many = edges.find((edge) => edge.cardinality === 'many_to_many');
		expect(many?.viaId).toBe(userRoles.id);
		expect(many?.strength).toBe('strong');
	});

	it('links a view to the table named in its definition', () => {
		const orders = table('orders', [column('id', 'PRI', 'order id')]);
		const view: CatalogObject = {
			...table('open_orders', []),
			kind: 'view',
			definition: 'select id from `orders` where status = 1'
		};
		const edges = buildViewEdges({ objects: [orders, view], foreignKeys: [], warnings: [] });
		expect(edges.map((edge) => [edge.fromId, edge.toId])).toEqual([[view.id, orders.id]]);
	});

	it('keeps inferred relationships that are not already foreign keys', () => {
		const users = table('users', [column('id', 'PRI', 'user')]);
		const orders = table('orders', [column('id', 'PRI'), column('user_id')]);
		const catalog: SchemaCatalog = {
			objects: [users, orders],
			foreignKeys: [fk('orders_user', orders, users, [['user_id', 'id']])],
			warnings: []
		};
		const graph = assembleGraph(
			catalog,
			[{ objectId: orders.id, columnName: 'user_id', comment: 'buyer', confidence: 'high' }],
			[
				{
					fromId: orders.id,
					toId: users.id,
					fromColumns: ['user_id'],
					toColumns: ['id'],
					strength: 'strong',
					cardinality: 'many_to_one',
					reason: 'duplicate of the foreign key',
					confidence: 'high'
				},
				{
					fromId: orders.id,
					toId: users.id,
					fromColumns: [],
					toColumns: [],
					strength: 'weak',
					cardinality: 'many_to_many',
					reason: 'notes mention shared owners',
					confidence: 'low'
				}
			]
		);
		expect(graph.nodes.find((node) => node.id === orders.id)?.columns[1]?.inferredComment).toBe(
			'buyer'
		);
		expect(graph.edges.filter((edge) => edge.origin === 'inferred')).toHaveLength(1);
		expect(graph.edges.some((edge) => edge.origin === 'physical')).toBe(true);
	});
});

function table(name: string, columns: CatalogObject['columns']): CatalogObject {
	return {
		id: objectId(scope.connectionId, scope.schema, name),
		connectionId: scope.connectionId,
		connectionName: scope.connectionName,
		engine: scope.engine,
		schema: scope.schema,
		name,
		kind: 'table',
		comment: '',
		definition: '',
		columns: columns.map((column, index) => ({ ...column, ordinal: index + 1 })),
		external: false
	};
}

function fk(
	name: string,
	from: CatalogObject,
	to: CatalogObject,
	pairs: Array<[string, string]>
): SchemaCatalog['foreignKeys'][number] {
	return {
		id: `${from.id}::${name}::${to.id}`,
		name,
		connectionId: scope.connectionId,
		fromId: from.id,
		toId: to.id,
		pairs: pairs.map(([fromColumn, toColumn]) => ({ fromColumn, toColumn }))
	};
}
