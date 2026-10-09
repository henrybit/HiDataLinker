import { describe, expect, it } from 'vitest';
import { formatBytes, formatDuration, formatElapsed, formatNumber } from './format';

describe('format helpers', () => {
	it('formats byte sizes', () => {
		expect(formatBytes(512)).toBe('512 B');
		expect(formatBytes(2048)).toBe('2 KB');
		expect(formatBytes(null)).toBe('—');
	});

	it('formats numbers and durations', () => {
		expect(formatNumber(1200)).toBe('1,200');
		expect(formatDuration(120)).toBe('120 ms');
		expect(formatDuration(2500)).toBe('2.50 s');
	});

	it('formats elapsed time past one minute', () => {
		expect(formatElapsed(2500)).toBe('2.50 s');
		expect(formatElapsed(125_000)).toBe('2m 05s');
		expect(formatElapsed(3_723_000)).toBe('1h 02m 03s');
	});
});
