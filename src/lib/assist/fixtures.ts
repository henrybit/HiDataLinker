import { objectId, type CatalogObject, type SchemaScope } from '$lib/analysis/types';
import type { AssistCatalog } from './catalog';

const scope: SchemaScope = {
	connectionId: 'c1',
	connectionName: 'shop-pg',
	engine: 'postgres',
	schema: 'shop'
};

function table(name: string, comment: string, columns: Array<[string, string, string?]>) {
	return {
		id: objectId('c1', 'shop', name),
		connectionId: 'c1',
		connectionName: 'shop-pg',
		engine: 'postgres',
		schema: 'shop',
		name,
		kind: 'table',
		comment,
		definition: '',
		external: false,
		columns: columns.map(([column, dataType, columnComment], index) => ({
			name: column,
			dataType,
			key: column === 'id' ? 'PRI' : '',
			comment: columnComment ?? '',
			ordinal: index + 1
		}))
	} satisfies CatalogObject;
}

/** Small shop schema shared by the assist tests. */
export function shopCatalog(): AssistCatalog {
	const customers = table('customers', '客户', [
		['id', 'bigint'],
		['name', 'text', '客户名称'],
		['city', 'text']
	]);
	const orders = table('orders', '订单', [
		['id', 'bigint'],
		['customer_id', 'bigint'],
		['amount', 'numeric(10,2)', '订单金额'],
		['created_at', 'timestamp', '下单时间']
	]);
	const items = table('order_items', '订单明细', [
		['id', 'bigint'],
		['order_id', 'bigint'],
		['product_id', 'bigint'],
		['quantity', 'integer']
	]);
	const products = table('products', '商品', [
		['id', 'bigint'],
		['title', 'text'],
		['price', 'numeric(10,2)']
	]);
	const audit = table('audit_log', '审计日志', [
		['id', 'bigint'],
		['action', 'text']
	]);
	return {
		scope,
		objects: [customers, orders, items, products, audit],
		foreignKeys: [
			{
				id: 'fk1',
				name: 'orders_customer_fk',
				connectionId: 'c1',
				fromId: orders.id,
				toId: customers.id,
				pairs: [{ fromColumn: 'customer_id', toColumn: 'id' }]
			},
			{
				id: 'fk2',
				name: 'items_order_fk',
				connectionId: 'c1',
				fromId: items.id,
				toId: orders.id,
				pairs: [{ fromColumn: 'order_id', toColumn: 'id' }]
			}
		],
		inferred: [
			{
				id: 'inf1',
				fromId: items.id,
				toId: products.id,
				fromColumns: ['product_id'],
				toColumns: ['id'],
				origin: 'inferred',
				strength: 'strong',
				cardinality: 'many_to_one',
				reasonCode: 'inferred',
				reason: 'name match',
				confidence: 'high'
			}
		],
		indexes: [
			{
				name: 'orders_created_idx',
				tableName: 'orders',
				unique: false,
				primary: false,
				indexType: 'btree',
				columns: ['created_at'],
				comment: ''
			}
		],
		warnings: [],
		loadedAt: Date.now()
	};
}
