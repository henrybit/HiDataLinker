import { describe, expect, it } from 'vitest';
import { summarizeToolResult } from './skill-client';

describe('summarizeToolResult', () => {
	it('returns error text when failed', () => {
		expect(summarizeToolResult('query', { ok: false, error: 'read-only' })).toBe('read-only');
	});

	it('stringifies success payloads', () => {
		expect(summarizeToolResult('list_tables', { ok: true, result: [{ name: 'orders' }] })).toContain(
			'orders'
		);
	});
});
