import { describe, expect, it } from 'vitest';
import type { QueryResult } from '$lib/api/types';
import {
	assistBlocker,
	describeContext,
	gridContext,
	maskSecrets,
	resultObject,
	type AssistContext
} from './context';

function context(overrides: Partial<AssistContext> = {}): AssistContext {
	return {
		origin: 'editor',
		connectionId: 'conn-1',
		connectionName: 'prod (10.0.0.8)',
		engine: 'postgres',
		connected: true,
		schema: 'shop',
		object: null,
		editor: { text: '', selection: null },
		grid: null,
		error: null,
		...overrides
	};
}

function result(columns: string[], rows: number, truncated = false): QueryResult {
	return {
		columns: columns.map((name) => ({ name, dataType: 'text' })),
		rows: Array.from({ length: rows }, () => columns.map(() => 'secret-row-value')),
		affectedRows: 0,
		durationMs: 1,
		truncated,
		statementKind: 'SELECT'
	} as unknown as QueryResult;
}

describe('assistBlocker', () => {
	it('reports the first missing prerequisite', () => {
		expect(assistBlocker(null, true)).toBe('no-connection');
		expect(assistBlocker(context({ connected: false }), true)).toBe('not-connected');
		expect(assistBlocker(context({ schema: null }), true)).toBe('no-schema');
		expect(assistBlocker(context(), false)).toBe('no-model');
		expect(assistBlocker(context(), true)).toBeNull();
	});
});

describe('maskSecrets', () => {
	it('redacts inline credentials', () => {
		expect(maskSecrets("CREATE USER app IDENTIFIED BY 'S3cret!'")).toBe(
			'CREATE USER app IDENTIFIED BY ***'
		);
		expect(maskSecrets("ALTER ROLE app WITH PASSWORD 'hunter2'")).toBe(
			'ALTER ROLE app WITH PASSWORD ***'
		);
		expect(maskSecrets('-- postgres://admin:topsecret@db.internal:5432/shop')).toBe(
			'-- postgres://***'
		);
		expect(maskSecrets('api_key=abc123 token: xyz')).toBe('api_key=*** token: ***');
		expect(maskSecrets(`secret = "a b c"`)).toBe('secret = ***');
		expect(maskSecrets('{"port": 4545, "token": "5f0c-9a1e"}')).toBe(
			'{"port": 4545, "token": "***"}'
		);
		expect(maskSecrets('Authorization: Bearer 5f0c9a1e-77aa-4b0c')).toBe(
			'Authorization: Bearer ***'
		);
	});

	it('leaves ordinary SQL alone', () => {
		const sql = "SELECT password_hash FROM users WHERE name = 'x'";
		expect(maskSecrets(sql)).toBe(sql);
	});
});

describe('resultObject', () => {
	it('finds the table behind a single-table select', () => {
		expect(resultObject('SELECT * FROM orders LIMIT 10', 'shop')).toEqual({
			schema: 'shop',
			name: 'orders',
			kind: 'table'
		});
		expect(resultObject('select id from "sales"."Order Lines"', null)).toEqual({
			schema: 'sales',
			name: 'Order Lines',
			kind: 'table'
		});
		expect(resultObject('SELECT * FROM [dbo].[Customers]', null)?.name).toBe('Customers');
	});

	it('gives up on joins and statements without a schema to resolve', () => {
		expect(resultObject('SELECT * FROM a JOIN b ON a.id = b.a_id', 'shop')).toBeNull();
		expect(resultObject('SELECT * FROM orders', null)).toBeNull();
		expect(resultObject('UPDATE orders SET x = 1', 'shop')).toBeNull();
	});
});

describe('describeContext', () => {
	it('describes the workspace without names, hosts, credentials or rows', () => {
		const grid = gridContext('SELECT * FROM orders', result(['id', 'amount'], 3), 'shop');
		const text = describeContext(
			context({
				object: { schema: 'shop', name: 'customers', kind: 'table' },
				editor: {
					text: "SELECT 1; -- mysql://root:pw@10.0.0.8/shop\nCREATE USER a IDENTIFIED BY 'x'",
					selection: null
				},
				grid
			})
		);
		expect(text).toContain('Engine: postgres');
		expect(text).toContain('Current schema/database: shop');
		expect(text).toContain('Selected table: shop.customers');
		expect(text).toContain('3 rows with columns: id, amount');
		expect(text).toContain('The grid result comes from shop.orders');
		expect(text).not.toContain('prod (');
		expect(text).not.toContain(':pw@');
		expect(text).not.toContain("'x'");
		expect(text).not.toContain('secret-row-value');
	});

	it('prefers the editor selection and includes the last error', () => {
		const text = describeContext(
			context({
				editor: { text: 'SELECT a FROM t;\nSELECT b FROM u;', selection: 'SELECT b FROM u;' },
				error: 'column "b" does not exist'
			})
		);
		expect(text).toContain('Selected text in the SQL editor:\nSELECT b FROM u;');
		expect(text).not.toContain('SELECT a FROM t');
		expect(text).toContain('The last run failed with:\ncolumn "b" does not exist');
	});
});
