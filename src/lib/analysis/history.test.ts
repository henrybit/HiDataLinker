import { describe, expect, it } from 'vitest';
import { analysisHistoryTitle, asRelationshipGraph } from './history';
import type { SchemaScope } from './types';

function scope(name: string, schema: string): SchemaScope {
	return { connectionId: name, connectionName: name, engine: 'mysql', schema };
}

describe('analysisHistoryTitle', () => {
	it('joins a few scopes and summarizes the rest', () => {
		expect(analysisHistoryTitle([scope('App', 'shop')])).toBe('App / shop');
		expect(
			analysisHistoryTitle([
				scope('App', 'shop'),
				scope('App', 'crm'),
				scope('Billing', 'pay'),
				scope('Billing', 'tax')
			])
		).toBe('App / shop, App / crm, Billing / pay +1');
	});
});

describe('asRelationshipGraph', () => {
	it('accepts a stored graph and fills missing warnings', () => {
		expect(asRelationshipGraph({ nodes: [], edges: [] })).toEqual({
			nodes: [],
			edges: [],
			warnings: []
		});
		expect(asRelationshipGraph({ nodes: [] })).toBeNull();
	});
});
