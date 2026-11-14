import { describe, expect, it } from 'vitest';
import type { DraftOutcome } from './draft';
import { followUpText, turnOf } from './followup';
import type { DraftResponse } from './prompt';

const response = (overrides: Partial<DraftResponse>): DraftResponse => ({
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
});

describe('follow-up turns', () => {
	it('keeps drafts and clarifications, skips everything else', () => {
		const draft = {
			kind: 'draft',
			sql: 'SELECT 1',
			response: response({ sql: 'SELECT 1' })
		} as DraftOutcome;
		const clarify = {
			kind: 'clarify',
			response: response({ status: 'clarify', missing: 'Which amount?' })
		} as DraftOutcome;
		expect(turnOf('q1', draft)).toEqual({ question: 'q1', kind: 'draft', answer: 'SELECT 1' });
		expect(turnOf('q2', clarify)).toEqual({
			question: 'q2',
			kind: 'clarify',
			answer: 'Which amount?'
		});
		expect(turnOf('q3', { kind: 'empty-catalog', schema: 's' })).toBeNull();
		expect(turnOf('q4', null)).toBeNull();
	});

	it('sends the last three turns, oldest first, with long SQL clipped', () => {
		const turns = ['a', 'b', 'c', 'd'].map((question) => ({
			question,
			kind: 'draft' as const,
			answer: question === 'd' ? 'x'.repeat(2000) : `SELECT '${question}'`
		}));
		const text = followUpText(turns);
		expect(text).not.toContain('Q: a');
		expect(text.indexOf('Q: b')).toBeLessThan(text.indexOf('Q: d'));
		expect(text).toContain("You drafted:\nSELECT 'c'");
		expect(text).toContain(`${'x'.repeat(1500)} …`);
		expect(followUpText([])).toBe('');
	});
});
