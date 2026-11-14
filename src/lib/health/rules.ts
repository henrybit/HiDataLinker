import type { HealthFinding, HealthSeverity } from '$lib/api/types';
import type { MessageKey } from '$lib/i18n/en';

export type HealthSource = 'dontDoThis' | 'indexes' | 'general';

export interface HealthRuleMeta {
	title: MessageKey;
	reason: MessageKey;
	source: HealthSource;
}

function rule(id: string, source: HealthSource): HealthRuleMeta {
	return {
		title: `health.rule.${id}.title` as MessageKey,
		reason: `health.rule.${id}.reason` as MessageKey,
		source
	};
}

export const healthRules: Record<string, HealthRuleMeta> = {
	timestamp_without_time_zone: rule('timestamp_without_time_zone', 'dontDoThis'),
	char_n: rule('char_n', 'dontDoThis'),
	varchar_n: rule('varchar_n', 'dontDoThis'),
	money_type: rule('money_type', 'dontDoThis'),
	timetz_type: rule('timetz_type', 'dontDoThis'),
	json_type: rule('json_type', 'general'),
	serial_column: rule('serial_column', 'dontDoThis'),
	missing_primary_key: rule('missing_primary_key', 'general'),
	unindexed_foreign_key: rule('unindexed_foreign_key', 'indexes'),
	duplicate_index: rule('duplicate_index', 'indexes'),
	invalid_index: rule('invalid_index', 'indexes'),
	unused_index: rule('unused_index', 'indexes'),
	rule_usage: rule('rule_usage', 'dontDoThis'),
	table_inheritance: rule('table_inheritance', 'dontDoThis'),
	uppercase_identifier: rule('uppercase_identifier', 'dontDoThis'),
	sequence_exhaustion: rule('sequence_exhaustion', 'general'),
	sql_ascii_encoding: rule('sql_ascii_encoding', 'dontDoThis')
};

export const severityOrder: HealthSeverity[] = ['error', 'warning', 'info'];

export interface HealthGroup {
	rule: string;
	/** Most severe finding in the group. */
	severity: HealthSeverity;
	meta: HealthRuleMeta | undefined;
	findings: HealthFinding[];
}

export function countBySeverity(findings: HealthFinding[]): Record<HealthSeverity, number> {
	const counts: Record<HealthSeverity, number> = { error: 0, warning: 0, info: 0 };
	for (const finding of findings) counts[finding.severity] += 1;
	return counts;
}

/** Groups findings by rule, most severe groups first; `only` keeps one severity. */
export function groupFindings(findings: HealthFinding[], only?: HealthSeverity): HealthGroup[] {
	const groups = new Map<string, HealthGroup>();
	for (const finding of findings) {
		if (only && finding.severity !== only) continue;
		const group = groups.get(finding.rule);
		if (group) {
			group.findings.push(finding);
			if (severityOrder.indexOf(finding.severity) < severityOrder.indexOf(group.severity)) {
				group.severity = finding.severity;
			}
		} else {
			groups.set(finding.rule, {
				rule: finding.rule,
				severity: finding.severity,
				meta: healthRules[finding.rule],
				findings: [finding]
			});
		}
	}
	return [...groups.values()].sort(
		(a, b) =>
			severityOrder.indexOf(a.severity) - severityOrder.indexOf(b.severity) ||
			a.rule.localeCompare(b.rule)
	);
}
