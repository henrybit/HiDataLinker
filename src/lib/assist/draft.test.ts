import { describe, expect, it } from 'vitest';
import type { ZodType } from 'zod';
import type { StructuredCaller } from '$lib/analysis/workflow';
import type { AssistContext } from './context';
import { cleanDraftSql, draftSql, DraftError, type DraftStep } from './draft';
import { shopCatalog } from './fixtures';
import { formatScope, type DraftResponse } from './prompt';
import { pickTables } from './select';
import { validateDraft } from './validate';

const objects = shopCatalog().objects;

describe('validateDraft', () => {
	it('accepts a join that matches the catalog', () => {
		const sql = `SELECT c.name, SUM(o.amount) AS total
FROM orders o
JOIN customers AS c ON c.id = o.customer_id
WHERE o.created_at >= now() - interval '7 days'
GROUP BY c.name`;
		expect(validateDraft(sql, objects, 'shop')).toEqual([]);
	});

	it('reports unknown tables and alias-qualified columns', () => {
		const sql = `SELECT o.total, p.name FROM orders o JOIN payments p ON p.order_id = o.id`;
		expect(validateDraft(sql, objects, 'shop')).toEqual([
			{ kind: 'table', name: 'payments' },
			{ kind: 'column', name: 'orders.total' }
		]);
	});

	it('checks columns the model declares', () => {
		expect(
			validateDraft('SELECT * FROM orders', objects, 'shop', ['orders.amount', 'orders.discount'])
		).toEqual([{ kind: 'column', name: 'orders.discount' }]);
	});

	it('ignores CTEs, subqueries, functions, other schemas and literals', () => {
		const sql = `WITH recent AS (SELECT o.id, o.customer_id FROM shop.orders o WHERE o.created_at > '2026-01-01')
SELECT r.id, EXTRACT(DAY FROM o2.created_at) AS d, t.n
FROM recent r
JOIN orders o2 ON o2.id = r.id
CROSS JOIN LATERAL (SELECT count(*) AS n FROM order_items i WHERE i.order_id = r.id) t
JOIN generate_series(1, 3) g ON true
JOIN pg_catalog.pg_class pc ON false
WHERE r.id IN (SELECT x.id FROM other_schema.thing x)
  AND 'FROM fake_table' <> ''`;
		expect(validateDraft(sql, objects, 'shop')).toEqual([]);
	});

	it('checks INSERT and UPDATE targets', () => {
		expect(validateDraft('INSERT INTO invoices (a) VALUES (1)', objects, 'shop')).toEqual([
			{ kind: 'table', name: 'invoices' }
		]);
		expect(validateDraft('UPDATE orders SET amount = 0 WHERE id = 1', objects, 'shop')).toEqual([]);
	});

	it('handles SQL Server and MySQL quoting', () => {
		expect(
			validateDraft('SELECT TOP (10) [o].[amount] FROM [shop].[orders] AS [o]', objects, 'shop')
		).toEqual([]);
		expect(validateDraft('SELECT `o`.`amout` FROM `orders` `o`', objects, 'shop')).toEqual([
			{ kind: 'column', name: 'orders.amout' }
		]);
	});
});

describe('pickTables', () => {
	const context: Pick<AssistContext, 'object' | 'editor' | 'grid' | 'error'> = {
		object: null,
		editor: null,
		grid: null,
		error: null
	};

	it('matches English names and pulls in join neighbours', () => {
		const scope = pickTables(shopCatalog(), 'order amount per customer', context, 3);
		expect(scope.tables.map((table) => table.name).sort()).toEqual([
			'customers',
			'order_items',
			'orders'
		]);
		expect(scope.others).toEqual(['products', 'audit_log']);
		expect(scope.relations.map((hint) => `${hint.from}->${hint.to}`)).toEqual([
			'orders->customers',
			'order_items->orders'
		]);
	});

	it('matches Chinese comments', () => {
		const scope = pickTables(shopCatalog(), '近 7 天每个客户的订单金额', context, 2);
		expect(scope.tables.map((table) => table.name).sort()).toEqual(['customers', 'orders']);
	});

	it('ranks the selected object and editor tables first', () => {
		const scope = pickTables(
			shopCatalog(),
			'how many',
			{
				...context,
				object: { schema: 'shop', name: 'audit_log', kind: 'table' },
				editor: { text: 'SELECT * FROM products', selection: null }
			},
			2
		);
		expect(scope.tables.map((table) => table.name)).toEqual(['audit_log', 'products']);
		expect(scope.reasons).toEqual({ audit_log: 'selected', products: 'editor' });
	});

	it('follows undeclared <table>_id joins by column name', () => {
		const catalog = { ...shopCatalog(), inferred: [] };
		const scope = pickTables(catalog, '每个商品的销量', context, 2);
		expect(scope.tables.map((table) => table.name).sort()).toEqual(['order_items', 'products']);
		expect(scope.relations).toEqual([
			{
				from: 'order_items',
				to: 'products',
				fromColumns: ['product_id'],
				toColumns: ['id'],
				origin: 'naming'
			}
		]);
		expect(formatScope('shop', scope, [])).toContain(
			'order_items.product_id = products.id (column name only, no constraint)'
		);
	});

	it('sends a small schema whole when nothing matches', () => {
		const scope = pickTables(shopCatalog(), 'xyz', context);
		expect(scope.tables).toHaveLength(5);
		expect(scope.reasons.orders).toBe('small-schema');
	});
});

describe('formatScope', () => {
	it('lists columns, indexes and both kinds of relations', () => {
		const catalog = shopCatalog();
		const scope = pickTables(catalog, 'order items with product title', {
			object: null,
			editor: null,
			grid: null,
			error: null
		});
		const text = formatScope('shop', scope, catalog.indexes);
		expect(text).toContain('orders (table) -- 订单');
		expect(text).toContain('  amount numeric(10,2) -- 订单金额');
		expect(text).toContain('  id bigint PK');
		expect(text).toContain('indexes: (created_at)');
		expect(text).toContain('orders.customer_id = customers.id (foreign key)');
		expect(text).toContain('order_items.product_id = products.id (inferred, high confidence)');
	});
});

describe('cleanDraftSql', () => {
	it('strips fences and Oracle semicolons', () => {
		expect(cleanDraftSql('```sql\nSELECT 1;\n```', 'postgres')).toBe('SELECT 1;');
		expect(cleanDraftSql('SELECT 1 FROM dual;', 'oracle')).toBe('SELECT 1 FROM dual');
	});
});

function response(overrides: Partial<DraftResponse>): DraftResponse {
	return {
		status: 'draft',
		sql: '',
		summary: '',
		tables: [],
		columns: [],
		relations: [],
		assumptions: [],
		candidates: [],
		missing: '',
		...overrides
	};
}

function scriptedCaller(replies: DraftResponse[], prompts: string[] = []): StructuredCaller {
	return {
		async complete<T>(schema: ZodType<T>, _system: string, human: string): Promise<T> {
			prompts.push(human);
			const next = replies.shift();
			if (!next) throw new Error('no more replies');
			return schema.parse(next);
		}
	};
}

const baseContext: AssistContext = {
	origin: 'editor',
	connectionId: 'c1',
	connectionName: 'shop-pg',
	engine: 'postgres',
	connected: true,
	schema: 'shop',
	object: null,
	editor: { text: '', selection: null },
	grid: null,
	error: null
};

describe('draftSql', () => {
	const loadCatalog = async () => ({ catalog: shopCatalog(), cached: false });

	it('returns a validated draft in one model call', async () => {
		const prompts: string[] = [];
		const phases: string[] = [];
		const outcome = await draftSql({
			question: 'order amount per customer',
			context: baseContext,
			locale: 'en',
			loadCatalog,
			onPhase: (phase) => phases.push(phase),
			caller: scriptedCaller(
				[
					response({
						sql: 'SELECT c.name, SUM(o.amount) FROM orders o JOIN customers c ON c.id = o.customer_id GROUP BY c.name',
						columns: ['customers.name', 'orders.amount']
					})
				],
				prompts
			)
		});
		expect(outcome.kind).toBe('draft');
		expect(phases).toEqual(['catalog', 'drafting', 'checking']);
		expect(prompts).toHaveLength(1);
		expect(prompts[0]).toContain('Question: order amount per customer');
		expect(prompts[0]).not.toContain('shop-pg');
	});

	it('repairs once, then rejects drafts that still invent columns', async () => {
		const prompts: string[] = [];
		const outcome = await draftSql({
			question: 'orders with discount',
			context: baseContext,
			locale: 'en',
			loadCatalog,
			caller: scriptedCaller(
				[
					response({ sql: 'SELECT o.discount FROM orders o' }),
					response({ sql: 'SELECT o.discount_rate FROM orders o' })
				],
				prompts
			)
		});
		expect(outcome.kind).toBe('rejected');
		if (outcome.kind === 'rejected') {
			expect(outcome.issues).toEqual([{ kind: 'column', name: 'orders.discount_rate' }]);
		}
		expect(prompts[1]).toContain('do not exist in the catalog: column orders.discount');
	});

	it('accepts a repaired draft', async () => {
		const outcome = await draftSql({
			question: 'order totals',
			context: baseContext,
			locale: 'en',
			loadCatalog,
			caller: scriptedCaller([
				response({ sql: 'SELECT o.total FROM orders o' }),
				response({ sql: 'SELECT o.amount FROM orders o LIMIT 100' })
			])
		});
		expect(outcome).toMatchObject({ kind: 'draft', repaired: true });
	});

	it('reports each step: catalog, scope, check and recheck', async () => {
		const steps: DraftStep[] = [];
		let catalogReads = 0;
		await draftSql({
			question: 'order totals',
			context: baseContext,
			locale: 'en',
			loadCatalog: async () => {
				catalogReads += 1;
				return { catalog: shopCatalog(), cached: true };
			},
			onStep: (step) => steps.push(step),
			caller: scriptedCaller([
				response({ sql: 'SELECT o.total FROM orders o' }),
				response({ sql: 'SELECT o.amount FROM orders o LIMIT 100' })
			])
		});
		expect(catalogReads).toBe(1);
		expect(steps.map((step) => step.kind)).toEqual(['catalog', 'scope', 'check', 'recheck']);
		expect(steps[0]).toEqual({ kind: 'catalog', cached: true, objects: 5, relations: 3 });
		expect(steps[1]).toMatchObject({
			kind: 'scope',
			tables: expect.arrayContaining([{ name: 'orders', reason: 'question' }]),
			relations: expect.arrayContaining(['orders.customer_id = customers.id'])
		});
		expect(steps[2]).toEqual({ kind: 'check', issues: [{ kind: 'column', name: 'orders.total' }] });
		expect(steps[3]).toEqual({ kind: 'recheck', issues: [] });
	});

	it('pins the tables the user picked after a clarification', async () => {
		const prompts: string[] = [];
		const steps: DraftStep[] = [];
		await draftSql({
			question: 'how many records',
			context: baseContext,
			locale: 'en',
			loadCatalog,
			pinned: ['audit_log'],
			onStep: (step) => steps.push(step),
			caller: scriptedCaller([response({ sql: 'SELECT COUNT(*) FROM audit_log' })], prompts)
		});
		expect(steps[1]).toMatchObject({
			kind: 'scope',
			tables: expect.arrayContaining([{ name: 'audit_log', reason: 'pinned' }])
		});
		expect(prompts[0]).toContain(
			'The user confirmed the question is about these tables: audit_log.'
		);
	});

	it('passes clarifications through', async () => {
		const outcome = await draftSql({
			question: 'refunds last month',
			context: baseContext,
			locale: 'en',
			loadCatalog,
			caller: scriptedCaller([
				response({ status: 'clarify', candidates: ['orders'], missing: 'No refund table' })
			])
		});
		expect(outcome).toMatchObject({ kind: 'clarify' });
	});

	it('reports an empty catalog without calling the model', async () => {
		const outcome = await draftSql({
			question: 'anything',
			context: baseContext,
			locale: 'en',
			loadCatalog: async () => ({ catalog: { ...shopCatalog(), objects: [] }, cached: false }),
			caller: scriptedCaller([])
		});
		expect(outcome).toEqual({ kind: 'empty-catalog', schema: 'shop' });
	});

	it('sends no passwords, connection strings, hosts or gateway tokens to the model', async () => {
		const sent: string[] = [];
		const token = '5f0c9a1e-77aa-4b0c-9d2e-0123456789ab';
		await draftSql({
			question: "orders for bob, password='hunter2'",
			context: {
				...baseContext,
				connectionName: 'prod-db (10.0.0.8)',
				editor: {
					text: [
						'-- postgres://admin:s3cret@10.0.0.8:5432/shop',
						`-- endpoint.json {"port": 4545, "token": "${token}"}`,
						"ALTER USER app WITH PASSWORD 'p@ss'",
						'SELECT o.id FROM orders o'
					].join('\n'),
					selection: null
				},
				error: `request failed: Authorization: Bearer ${token}`
			},
			locale: 'en',
			loadCatalog,
			caller: {
				async complete<T>(schema: ZodType<T>, system: string, human: string): Promise<T> {
					sent.push(system, human);
					return schema.parse(response({ sql: 'SELECT o.id FROM orders o' }));
				}
			}
		});
		const request = sent.join('\n');
		for (const secret of ['hunter2', 's3cret', token, 'p@ss', 'prod-db', '10.0.0.8:5432']) {
			expect(request).not.toContain(secret);
		}
	});

	it('labels catalog and model failures', async () => {
		await expect(
			draftSql({
				question: 'x',
				context: baseContext,
				locale: 'en',
				loadCatalog: async () => {
					throw new Error('permission denied');
				},
				caller: scriptedCaller([])
			})
		).rejects.toMatchObject({ stage: 'catalog', message: 'permission denied' });
		await expect(
			draftSql({
				question: 'x',
				context: baseContext,
				locale: 'en',
				loadCatalog,
				caller: scriptedCaller([])
			})
		).rejects.toBeInstanceOf(DraftError);
	});
});
