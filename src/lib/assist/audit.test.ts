import { describe, expect, it } from 'vitest';
import { draftDetail, findAppliedDraft, needsDraftConfirm, newDraftId, runDetail } from './audit';

describe('draft audit helpers', () => {
	it('makes short ids and masked one-line details', () => {
		expect(newDraftId()).toMatch(/^[0-9a-f]{8}$/);
		expect(draftDetail('ab12cd34', "reset\n  user password = 'hunter2' for bob")).toBe(
			'ab12cd34 · reset user password = *** for bob'
		);
		expect(draftDetail('ab12cd34', 'x'.repeat(300))).toHaveLength(8 + 3 + 120);
	});

	it('finds an applied draft in the editor text, ignoring whitespace and case', () => {
		const drafts = [
			{ id: 'old', sql: 'SELECT 1' },
			{ id: 'new', sql: 'SELECT o.id\nFROM orders o;' }
		];
		expect(findAppliedDraft('-- mine\nselect o.id   from orders o', drafts)?.id).toBe('new');
		expect(findAppliedDraft('SELECT o.id FROM orders o WHERE o.id = 1', drafts)?.id).toBe('new');
		expect(findAppliedDraft('SELECT o.amount FROM orders o', drafts)).toBeNull();
		expect(findAppliedDraft('SELECT 1', drafts)?.id).toBe('old');
		expect(findAppliedDraft('  ', drafts)).toBeNull();
		expect(runDetail(drafts[1])).toBe('assist:new');
		expect(runDetail(null)).toBeNull();
	});

	it('asks for the stronger confirmation only for drafted writes', () => {
		const draft = { id: 'd1', sql: 'DELETE FROM orders WHERE id = 1' };
		expect(needsDraftConfirm(draft.sql, 'postgres', 'run', draft)).toBe(true);
		expect(needsDraftConfirm(draft.sql, 'postgres', 'explain', draft)).toBe(false);
		expect(needsDraftConfirm(draft.sql, 'mssql', 'explain', draft)).toBe(true);
		expect(needsDraftConfirm('SELECT 1', 'postgres', 'run', { id: 'd2', sql: 'SELECT 1' })).toBe(
			false
		);
		expect(needsDraftConfirm(draft.sql, 'postgres', 'run', null)).toBe(false);
	});
});
