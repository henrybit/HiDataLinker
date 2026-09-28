import { describe, expect, it } from 'vitest';
import { objectId, type SchemaCatalog } from './types';
import { inferRelationships, type StructuredCaller } from './workflow';

describe('inferRelationships', () => {
	it('runs comment batches and then relationship inference through LangGraph', async () => {
		const catalog = sampleCatalog();
		const calls: string[] = [];
		const caller = {
			async complete(_schema: unknown, _system: string, human: string) {
				calls.push(human.slice(0, 16));
				if (human.startsWith('TASK: comments')) {
					const ids = [...human.matchAll(/^(o\d+(?:#[^\s|]+)?)/gm)].map((match) => match[1]);
					return {
						comments: ids.map((id) => ({
							id,
							comment: `meaning of ${id}`,
							confidence: 'medium' as const
						}))
					};
				}
				return {
					relations: [
						{
							fromId: 'missing',
							toId: 'o2',
							fromColumns: [],
							toColumns: [],
							strength: 'weak' as const,
							cardinality: 'many_to_one' as const,
							reason: 'unknown endpoint',
							confidence: 'low' as const
						},
						{
							fromId: 'o2',
							toId: 'o1',
							fromColumns: ['note'],
							toColumns: ['id'],
							strength: 'weak' as const,
							cardinality: 'many_to_many' as const,
							reason: 'documents mention a loose link',
							confidence: 'low' as const
						}
					]
				};
			}
		} as StructuredCaller;

		const graph = await inferRelationships({
			catalog,
			documents: '# billing.md\norders may belong to several accounts',
			locale: 'en',
			caller,
			commentBatchSize: 2
		});

		expect(calls.filter((call) => call.startsWith('TASK: comments'))).toHaveLength(2);
		expect(calls.filter((call) => call.startsWith('TASK: relations'))).toHaveLength(1);
		const orders = graph.nodes.find((node) => node.name === 'orders');
		expect(orders?.columns.every((column) => column.inferredComment)).toBe(true);
		expect(graph.edges.filter((edge) => edge.origin === 'inferred')).toHaveLength(1);
		expect(graph.edges.find((edge) => edge.origin === 'inferred')?.cardinality).toBe(
			'many_to_many'
		);
	});
});

function sampleCatalog(): SchemaCatalog {
	const users = objectId('c1', 'shop', 'users');
	const orders = objectId('c1', 'shop', 'orders');
	return {
		objects: [
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
				external: false,
				columns: [{ name: 'id', dataType: 'int', key: 'PRI', comment: 'primary key', ordinal: 1 }]
			},
			{
				id: orders,
				connectionId: 'c1',
				connectionName: 'App',
				engine: 'mysql',
				schema: 'shop',
				name: 'orders',
				kind: 'table',
				comment: 'order header',
				definition: '',
				external: false,
				columns: [
					{ name: 'id', dataType: 'int', key: 'PRI', comment: '', ordinal: 1 },
					{ name: 'user_id', dataType: 'int', key: '', comment: '', ordinal: 2 },
					{ name: 'note', dataType: 'text', key: '', comment: '', ordinal: 3 }
				]
			}
		],
		foreignKeys: [],
		warnings: []
	};
}
