import type { ChatMessage } from '$lib/stores/agent-chat.svelte';
import { getLocale } from '$lib/i18n/i18n.svelte';

/** Plain text used when copying a chat message. */
export function messageCopyText(message: ChatMessage): string {
	if (message.kind === 'tool') {
		const summary =
			typeof message.meta?.summary === 'string' ? message.meta.summary.trim() : '';
		const body = message.content.trim();
		if (summary && summary !== body) {
			return [body, summary].filter(Boolean).join('\n\n');
		}
		return body || summary;
	}
	return message.content;
}

export function messageDurationMs(message: ChatMessage): number | null {
	const value = message.meta?.durationMs;
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

/** Format an ISO timestamp for the chat meta row (locale-aware). */
export function formatMessageTime(iso: string): string {
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return '';
	const locale = getLocale() === 'zh' ? 'zh-CN' : 'en-US';
	return new Intl.DateTimeFormat(locale, {
		hour: '2-digit',
		minute: '2-digit',
		second: '2-digit',
		hour12: false
	}).format(date);
}

/** Human-readable duration for tool calls and total run time. */
export function formatDurationMs(ms: number): string {
	if (!Number.isFinite(ms) || ms < 0) return '';
	if (ms < 1000) return `${Math.round(ms)}ms`;
	const seconds = ms / 1000;
	if (seconds < 60) {
		return seconds < 10 ? `${seconds.toFixed(1)}s` : `${Math.round(seconds)}s`;
	}
	const minutes = Math.floor(seconds / 60);
	const rem = Math.round(seconds % 60);
	return rem > 0 ? `${minutes}m ${rem}s` : `${minutes}m`;
}
