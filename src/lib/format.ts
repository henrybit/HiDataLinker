import { localeTag } from '$lib/i18n/i18n.svelte';

export function formatBytes(bytes?: number | null): string {
	if (bytes == null) return '—';
	if (bytes < 1024) return `${bytes} B`;
	const units = ['KB', 'MB', 'GB', 'TB'];
	let value = bytes / 1024;
	let unit = 0;
	while (value >= 1024 && unit < units.length - 1) {
		value /= 1024;
		unit += 1;
	}
	const digits = value >= 10 || Number.isInteger(Number(value.toFixed(1))) ? 0 : 1;
	return `${value.toFixed(digits)} ${units[unit]}`;
}

export function formatNumber(value?: number | null): string {
	if (value == null) return '—';
	return new Intl.NumberFormat(localeTag()).format(value);
}

export function formatDuration(ms: number): string {
	if (ms < 1000) return `${ms} ms`;
	return `${(ms / 1000).toFixed(2)} s`;
}

/** Short durations stay with `formatDuration`. Longer runs use hours and minutes. */
export function formatElapsed(ms: number): string {
	const rounded = Math.max(0, Math.round(ms));
	if (rounded < 60_000) return formatDuration(rounded);
	const totalSeconds = Math.floor(rounded / 1000);
	const hours = Math.floor(totalSeconds / 3600);
	const minutes = Math.floor((totalSeconds % 3600) / 60);
	const seconds = totalSeconds % 60;
	const clock = `${minutes.toString().padStart(2, '0')}m ${seconds.toString().padStart(2, '0')}s`;
	return hours > 0 ? `${hours}h ${clock}` : `${minutes}m ${seconds.toString().padStart(2, '0')}s`;
}

let counter = 0;
export function uid(prefix = 'id'): string {
	counter += 1;
	return `${prefix}-${Date.now().toString(36)}-${counter}`;
}
