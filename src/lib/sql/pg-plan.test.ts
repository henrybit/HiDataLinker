import { describe, expect, it } from 'vitest';
import analyzeFixture from './fixtures/pg-plan-analyze.json';
import plainFixture from './fixtures/pg-plan-plain.json';
import { estimateFactor, nodeShare, nodeTitle, parsePgPlan } from './pg-plan';

describe('parsePgPlan', () => {
	it('builds the tree with exclusive times from EXPLAIN ANALYZE', () => {
		const plan = parsePgPlan(JSON.stringify(analyzeFixture));
		expect(plan.analyzed).toBe(true);
		expect(plan.executionTime).toBeCloseTo(2.256);
		expect(plan.planningTime).toBeCloseTo(0.372);
		expect(plan.nodes.map((node) => node.title)).toEqual([
			'Sort',
			'HashAggregate',
			'Hash Join',
			'Seq Scan on ex.o',
			'Hash',
			'Seq Scan on ex.c'
		]);
		const join = plan.nodes[2];
		expect(join.exclusiveTime).toBeCloseTo(2.085 - 1.071 - 0.091);
		expect(join.details).toContain('Hash Cond: (o.cid = c.id)');
		expect(plan.nodes.find((node) => node.id === plan.hottestId)?.title).toBe('Seq Scan on ex.o');
	});

	it('flags large row misestimates only', () => {
		const plan = parsePgPlan(JSON.stringify(analyzeFixture));
		const flagged = plan.nodes.filter((node) => node.misestimated).map((node) => node.title);
		expect(flagged).toEqual(['Hash Join']);
		expect(plan.nodes[2].underestimated).toBe(false);
	});

	it('falls back to cost when the plan was not executed', () => {
		const plan = parsePgPlan(JSON.stringify(plainFixture));
		expect(plan.analyzed).toBe(false);
		expect(plan.executionTime).toBeNull();
		expect(plan.nodes.every((node) => node.exclusiveTime === null)).toBe(true);
		expect(plan.hottestId).not.toBeNull();
		const shares = plan.nodes.reduce((sum, node) => sum + nodeShare(plan, node), 0);
		expect(shares).toBeCloseTo(1);
	});

	it('rejects documents without a plan', () => {
		expect(() => parsePgPlan('[{}]')).toThrow();
	});
});

describe('nodeTitle', () => {
	it('names joins, aggregates, index scans and modify nodes like psql', () => {
		expect(nodeTitle({ 'Node Type': 'Hash Join', 'Join Type': 'Left' })).toBe('Hash Left Join');
		expect(nodeTitle({ 'Node Type': 'Nested Loop', 'Join Type': 'Anti' })).toBe(
			'Nested Loop Anti Join'
		);
		expect(nodeTitle({ 'Node Type': 'Aggregate', Strategy: 'Sorted' })).toBe('GroupAggregate');
		expect(
			nodeTitle({
				'Node Type': 'Index Scan',
				'Index Name': 'o_cid_idx',
				'Relation Name': 'o',
				Alias: 'o2'
			})
		).toBe('Index Scan using o_cid_idx on o o2');
		expect(
			nodeTitle({ 'Node Type': 'ModifyTable', Operation: 'Delete', 'Relation Name': 'o' })
		).toBe('Delete on o');
		expect(
			nodeTitle({ 'Node Type': 'CTE Scan', 'CTE Name': 'recent', 'Subplan Name': 'InitPlan 1' })
		).toBe('InitPlan 1 → CTE Scan on recent');
	});
});

describe('estimateFactor', () => {
	it('is symmetric and tolerates zero rows', () => {
		expect(estimateFactor(10, 100)).toBe(10);
		expect(estimateFactor(100, 10)).toBe(10);
		expect(estimateFactor(500, 0)).toBe(500);
	});
});
