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

		const events: string[] = [];
		const graph = await inferRelationships({
			catalog,
			documents: '# billing.md\norders may belong to several accounts',
			locale: 'en',
			caller,
			commentBatchSize: 2,
			onLog: (event) => events.push(event.type)
		});

		expect(calls.filter((call) => call.startsWith('TASK: comments'))).toHaveLength(2);
		expect(calls.filter((call) => call.startsWith('TASK: relations'))).toHaveLength(1);
		expect(events).toEqual([
			'comments-batch',
			'comments-batch-done',
			'comments-batch',
			'comments-batch-done',
			'relations-start',
			'relations-done'
		]);
		const orders = graph.nodes.find((node) => node.name === 'orders');
		expect(orders?.columns.every((column) => column.inferredComment)).toBe(true);
		expect(graph.edges.filter((edge) => edge.origin === 'inferred')).toHaveLength(1);
		expect(graph.edges.find((edge) => edge.origin === 'inferred')?.cardinality).toBe(
			'many_to_many'
		);
	});

	it('keeps inferring relationships when a comment batch fails', async () => {
		let commentCalls = 0;
		const caller = {
			async complete(_schema: unknown, _system: string, human: string) {
				if (human.startsWith('TASK: comments')) {
					commentCalls += 1;
					if (commentCalls === 1) {
						throw Object.assign(new Error('schema mismatch'), {
							status: 422,
							llmOutput: '{"comments":"bad"}',
							request_id: 'req_comment'
						});
					}
					return { comments: [] };
				}
				return {
					relations: [
						{
							fromId: 'o2',
							toId: 'o1',
							fromColumns: ['user_id'],
							toColumns: ['id'],
							strength: 'strong' as const,
							cardinality: 'many_to_one' as const,
							reason: 'user_id points at users.id',
							confidence: 'high' as const
						}
					]
				};
			}
		} as StructuredCaller;

		const events: Array<{ type: string; detail?: string }> = [];
		const graph = await inferRelationships({
			catalog: sampleCatalog(),
			documents: '',
			locale: 'zh',
			caller,
			commentBatchSize: 2,
			onLog: (event) =>
				events.push({
					type: event.type,
					detail: event.type === 'comments-batch-failed' ? event.detail : undefined
				})
		});

		expect(events.map((event) => event.type)).toEqual([
			'comments-batch',
			'comments-batch-failed',
			'comments-batch',
			'comments-batch-done',
			'relations-start',
			'relations-done'
		]);
		expect(events[1]?.detail).toContain('schema mismatch');
		expect(events[1]?.detail).toContain('HTTP 422');
		expect(events[1]?.detail).toContain('request-id req_comment');
		expect(events[1]?.detail).toContain('output: {"comments":"bad"}');
		expect(graph.edges.filter((edge) => edge.origin === 'inferred')).toHaveLength(1);
		expect(graph.warnings.some((warning) => warning.code === 'comments')).toBe(true);
	});

	it('keeps the physical graph when relationship inference fails', async () => {
		const catalog = sampleCatalog();
		const users = catalog.objects[0]?.id ?? '';
		const orders = catalog.objects[1]?.id ?? '';
		catalog.foreignKeys = [
			{
				id: 'fk1',
				name: 'fk_orders_user',
				connectionId: 'c1',
				fromId: orders,
				toId: users,
				pairs: [{ fromColumn: 'user_id', toColumn: 'id' }]
			}
		];
		const caller = {
			async complete(_schema: unknown, _system: string, human: string) {
				if (human.startsWith('TASK: comments')) return { comments: [] };
				throw new Error('model request timed out\noperation timed out');
			}
		} as StructuredCaller;
		const events: Array<{ type: string; detail?: string }> = [];
		const graph = await inferRelationships({
			catalog,
			documents: '',
			locale: 'zh',
			caller,
			onLog: (event) =>
				events.push({
					type: event.type,
					detail: event.type === 'relations-failed' ? event.detail : undefined
				})
		});
		expect(events.map((event) => event.type)).toContain('relations-failed');
		expect(events.find((event) => event.type === 'relations-failed')?.detail).toContain(
			'model request timed out'
		);
		expect(graph.nodes).toHaveLength(2);
		expect(graph.edges.filter((edge) => edge.origin === 'physical')).toHaveLength(1);
		expect(graph.edges.filter((edge) => edge.origin === 'inferred')).toHaveLength(0);
		expect(graph.warnings.some((warning) => warning.code === 'relations')).toBe(true);
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
