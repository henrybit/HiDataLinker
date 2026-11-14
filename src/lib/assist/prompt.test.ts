import { describe, expect, it } from 'vitest';
import { analysisResponseSchema } from '$lib/analysis/llm';
import { draftResponseSchema } from './prompt';

describe('draftResponseSchema', () => {
	it('accepts a draft that leaves out the clarify-only fields', () => {
		const parsed = draftResponseSchema.parse({
			status: 'draft',
			sql: "SELECT COUNT(*) FROM t_users WHERE provider = 'google'",
			summary: 'Counts Google accounts.',
			tables: ['t_users'],
			columns: ['t_users.provider'],
			relations: [],
			assumptions: []
		});
		expect(parsed.missing).toBe('');
		expect(parsed.candidates).toEqual([]);
	});

	it('fills null or missing fields of a clarification with empty values', () => {
		const parsed = draftResponseSchema.parse({
			status: 'clarify',
			sql: null,
			summary: 'Which table records the sign-in provider?',
			candidates: ['t_users']
		});
		expect(parsed.sql).toBe('');
		expect(parsed.tables).toEqual([]);
		expect(parsed.missing).toBe('');
	});

	it('still rejects a response without a status', () => {
		expect(() => draftResponseSchema.parse({ sql: 'SELECT 1' })).toThrow();
	});

	it('still asks the model for every field', () => {
		const json = analysisResponseSchema(draftResponseSchema) as { required: string[] };
		expect(json.required).toEqual(expect.arrayContaining(['status', 'sql', 'missing']));
	});
});
