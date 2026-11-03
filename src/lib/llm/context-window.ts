/** Default context window when a profile omits the field (128K tokens). */
export const DEFAULT_CONTEXT_WINDOW = 128_000;

const MIN_CONTEXT_WINDOW = 1_024;
const MAX_CONTEXT_WINDOW = 2_000_000;

/** Normalize a stored / form value into a sane token budget. */
export function normalizeContextWindow(value: unknown): number {
	const n =
		typeof value === 'number'
			? value
			: typeof value === 'string'
				? Number(value.trim().replace(/[,_\s]/g, ''))
				: NaN;
	if (!Number.isFinite(n)) return DEFAULT_CONTEXT_WINDOW;
	const rounded = Math.round(n);
	if (rounded < MIN_CONTEXT_WINDOW) return DEFAULT_CONTEXT_WINDOW;
	return Math.min(rounded, MAX_CONTEXT_WINDOW);
}

/**
 * Rough token estimate for mixed CJK / Latin text.
 * CJK ≈ 1.5 chars/token; Latin ≈ 4 chars/token.
 */
export function estimateTokensFromText(text: string): number {
	if (!text) return 0;
	let cjk = 0;
	let other = 0;
	for (const ch of text) {
		const code = ch.codePointAt(0) ?? 0;
		if (code >= 0x2e80) cjk += 1;
		else other += 1;
	}
	return Math.ceil(cjk / 1.5 + other / 4);
}

/** Compact label: 128K, 12.4K, 850. */
export function formatTokenCount(tokens: number): string {
	const n = Math.max(0, Math.round(tokens));
	if (n >= 10_000) return `${Math.round(n / 1000)}K`;
	if (n >= 1000) {
		const k = n / 1000;
		return `${k >= 10 ? Math.round(k) : Math.round(k * 10) / 10}K`;
	}
	return String(n);
}

export function contextUsageRatio(used: number, max: number): number {
	if (!Number.isFinite(used) || !Number.isFinite(max) || max <= 0) return 0;
	return Math.min(1, Math.max(0, used / max));
}

export type ContextUsageLevel = 'ok' | 'warn' | 'critical';

export function contextUsageLevel(ratio: number): ContextUsageLevel {
	if (ratio >= 0.9) return 'critical';
	if (ratio >= 0.7) return 'warn';
	return 'ok';
}
