import { describe, expect, it } from 'vitest';
import { actResponseSchema, planResponseSchema, synthesizeResponseSchema } from './prompts';

describe('data-analyst prompts schemas', () => {
	it('accepts a compact plan', () => {
		const parsed = planResponseSchema.parse({
			steps: [{ title: 'List tables', goal: 'Find order tables', toolHint: 'list_tables' }]
		});
		expect(parsed.steps).toHaveLength(1);
	});

	it('accepts a query tool act', () => {
		const parsed = actResponseSchema.parse({
			done: false,
			note: 'Run count',
			tool: 'query',
			args: { connectionId: 'c1', sql: 'SELECT 1' }
		});
		expect(parsed.tool).toBe('query');
	});

	it('accepts markdown synthesis', () => {
		const parsed = synthesizeResponseSchema.parse({
			markdown: '## Summary\n\nLooks fine.'
		});
		expect(parsed.markdown).toContain('Summary');
	});
});
