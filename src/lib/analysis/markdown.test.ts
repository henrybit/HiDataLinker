import { afterEach, describe, expect, it } from 'vitest';
import { setLocale } from '$lib/i18n/i18n.svelte';
import { renderRelationshipMarkdown } from './markdown';
import { objectId, type RelationshipGraph } from './types';

afterEach(() => {
	setLocale('en');
});

describe('renderRelationshipMarkdown', () => {
	it('describes objects, comments, and relationships', () => {
		setLocale('zh');
		const markdown = renderRelationshipMarkdown(sampleGraph(), '2026-09-28T09:00:00.000Z');
		expect(markdown).toContain('# 关系网分析');
		expect(markdown).toContain('2026-09-28T09:00:00.000Z');
		expect(markdown).toContain('2 个对象 · 1 条关系');
		expect(markdown).toContain('## 关系图');
		expect(markdown).toContain('```mermaid');
		expect(markdown).toContain('flowchart LR');
		expect(markdown).toContain('  n1["shop.orders"]');
		expect(markdown).toContain('  n2["shop.users"]');
		expect(markdown).toContain('  n1 -->|"user_id → id · N:1"| n2');
		expect(markdown).toContain('### App / shop');
		expect(markdown).toContain('#### orders');
		expect(markdown).toContain('订单头');
		expect(markdown).toContain('| user_id | int |  | 购买人 (推断, 中置信) |');
		expect(markdown).toContain(
			'| shop.orders (user_id) | shop.users (id) | 物理 · 强关系 | N:1 | 已声明的外键 |'
		);
	});

	it('escapes table punctuation in names', () => {
		setLocale('en');
		const graph = sampleGraph();
		graph.nodes[1]!.name = 'order|header';
		const markdown = renderRelationshipMarkdown(graph, 'now');
		expect(markdown).toContain('#### order\\|header');
		expect(markdown).not.toContain('#### order|header');
		expect(markdown).toContain('["shop.order/header"]');
	});

	it('keeps the physical network when inference is missing', () => {
		setLocale('zh');
		const graph = sampleGraph();
		graph.warnings = [
			{ code: 'comments', connectionName: '', schema: '' },
			{
				code: 'relations',
				connectionName: '',
				schema: '',
				detail: 'model request timed out'
			}
		];
		const markdown = renderRelationshipMarkdown(graph, 'now');
		expect(markdown).toContain('部分注释没有推断出来');
		expect(markdown).toContain('关联推断没有完成');
		expect(markdown).toContain('model request timed out');
		expect(markdown).toContain('已声明的外键');
		expect(markdown).toContain('```mermaid');
	});
});

function sampleGraph(): RelationshipGraph {
	const users = objectId('c1', 'shop', 'users');
	const orders = objectId('c1', 'shop', 'orders');
	return {
		warnings: [],
		nodes: [
			{
				id: users,
				connectionId: 'c1',
				connectionName: 'App',
				engine: 'mysql',
				schema: 'shop',
				name: 'users',
				kind: 'table',
				comment: 'account',
				definition: '',
				columns: [{ name: 'id', dataType: 'int', key: 'PRI', comment: 'primary key', ordinal: 1 }],
				external: false
			},
			{
				id: orders,
				connectionId: 'c1',
				connectionName: 'App',
				engine: 'mysql',
				schema: 'shop',
				name: 'orders',
				kind: 'table',
				comment: '订单头',
				definition: '',
				columns: [
					{ name: 'id', dataType: 'int', key: 'PRI', comment: '', ordinal: 1 },
					{
						name: 'user_id',
						dataType: 'int',
						key: '',
						comment: '',
						ordinal: 2,
						inferredComment: '购买人',
						inferenceConfidence: 'medium'
					}
				],
				external: false
			}
		],
		edges: [
			{
				id: 'fk',
				fromId: orders,
				toId: users,
				fromColumns: ['user_id'],
				toColumns: ['id'],
				origin: 'physical',
				strength: 'strong',
				cardinality: 'many_to_one',
				reasonCode: 'foreign-key',
				reason: 'foreign key'
			}
		]
	};
}
