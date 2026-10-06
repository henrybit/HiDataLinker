import { describe, expect, it } from 'vitest';
import { asMigrationLogs, migrationHistoryTitle } from './history';

describe('migrationHistoryTitle', () => {
	it('joins source and target labels', () => {
		expect(migrationHistoryTitle('Local', 'shop', 'Remote', 'shop_copy')).toBe(
			'Local / shop → Remote / shop_copy'
		);
	});
});

describe('asMigrationLogs', () => {
	it('keeps progress events with phase and message', () => {
		const logs = asMigrationLogs([
			{ phase: 'dump', level: 'info', current: 1, total: 2, message: 'table a' },
			{ phase: 1, message: 'bad' },
			null
		]);
		expect(logs).toHaveLength(1);
		expect(logs[0]?.phase).toBe('dump');
	});
});
