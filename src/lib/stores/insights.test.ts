import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

const { isReportStale } = await import('./insights.svelte');

const report = (generatedAt: string) => ({
	generatedAt,
	locale: 'en' as const,
	connections: [],
	items: []
});

describe('isReportStale', () => {
	const now = Date.parse('2026-11-10T12:00:00Z');

	it('treats a missing report as stale', () => {
		expect(isReportStale(null, now)).toBe(true);
	});

	it('keeps reports younger than 24h', () => {
		expect(isReportStale(report('2026-11-10T00:00:00Z'), now)).toBe(false);
	});

	it('regenerates reports older than 24h or with a bad timestamp', () => {
		expect(isReportStale(report('2026-11-09T11:00:00Z'), now)).toBe(true);
		expect(isReportStale(report('not-a-date'), now)).toBe(true);
	});
});
