import { describe, expect, it } from 'vitest';
import {
	DEFAULT_CONTEXT_WINDOW,
	contextUsageLevel,
	contextUsageRatio,
	estimateTokensFromText,
	formatTokenCount,
	normalizeContextWindow
} from './context-window';

describe('normalizeContextWindow', () => {
	it('defaults missing or tiny values to 128K', () => {
		expect(normalizeContextWindow(undefined)).toBe(DEFAULT_CONTEXT_WINDOW);
		expect(normalizeContextWindow(100)).toBe(DEFAULT_CONTEXT_WINDOW);
		expect(normalizeContextWindow('')).toBe(DEFAULT_CONTEXT_WINDOW);
	});

	it('accepts numeric strings and clamps the upper bound', () => {
		expect(normalizeContextWindow('200000')).toBe(200_000);
		expect(normalizeContextWindow(9_999_999)).toBe(2_000_000);
	});
});

describe('estimateTokensFromText', () => {
	it('returns 0 for empty input', () => {
		expect(estimateTokensFromText('')).toBe(0);
	});

	it('estimates latin and cjk differently', () => {
		expect(estimateTokensFromText('abcd')).toBe(1);
		expect(estimateTokensFromText('你好世界')).toBeGreaterThan(1);
	});
});

describe('formatTokenCount', () => {
	it('formats thousands compactly', () => {
		expect(formatTokenCount(850)).toBe('850');
		expect(formatTokenCount(12_400)).toBe('12K');
		expect(formatTokenCount(128_000)).toBe('128K');
	});
});

describe('contextUsageLevel', () => {
	it('maps ratios to severity', () => {
		expect(contextUsageLevel(contextUsageRatio(10, 100))).toBe('ok');
		expect(contextUsageLevel(0.75)).toBe('warn');
		expect(contextUsageLevel(0.95)).toBe('critical');
	});
});
