import { describe, expect, it } from 'vitest';
import type { HealthFinding } from '$lib/api/types';
import { countBySeverity, groupFindings, healthRules } from './rules';
import { en } from '$lib/i18n/en';
import { zh } from '$lib/i18n/zh';

function finding(rule: string, severity: HealthFinding['severity'], object: string): HealthFinding {
	return { rule, severity, object, facts: '', fixSql: null };
}

const findings = [
	finding('unused_index', 'info', 'a_idx'),
	finding('sequence_exhaustion', 'warning', 'b_seq'),
	finding('sequence_exhaustion', 'error', 'a_seq'),
	finding('char_n', 'warning', 't.code'),
	finding('unused_index', 'info', 'b_idx')
];

describe('groupFindings', () => {
	it('groups by rule and orders groups by their most severe finding', () => {
		const groups = groupFindings(findings);
		expect(groups.map((group) => [group.rule, group.severity, group.findings.length])).toEqual([
			['sequence_exhaustion', 'error', 2],
			['char_n', 'warning', 1],
			['unused_index', 'info', 2]
		]);
	});

	it('filters to one severity', () => {
		const groups = groupFindings(findings, 'warning');
		expect(groups.map((group) => [group.rule, group.findings.length])).toEqual([
			['char_n', 1],
			['sequence_exhaustion', 1]
		]);
	});
});

describe('countBySeverity', () => {
	it('counts each severity', () => {
		expect(countBySeverity(findings)).toEqual({ error: 1, warning: 2, info: 2 });
	});
});

describe('healthRules', () => {
	it('has a title and reason in every locale', () => {
		for (const meta of Object.values(healthRules)) {
			for (const messages of [en, zh] as Record<string, string>[]) {
				expect(messages[meta.title], meta.title).toBeTruthy();
				expect(messages[meta.reason], meta.reason).toBeTruthy();
			}
		}
	});
});
