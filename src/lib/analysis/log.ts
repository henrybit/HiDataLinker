export type AnalysisStage = 'start' | 'catalog' | 'comments' | 'relations' | 'done';

export type CatalogQueryName = 'objects' | 'columns' | 'foreignKeys' | 'views';

export type AnalysisLogLevel = 'info' | 'success' | 'warning' | 'error';

export type AnalysisLogEvent =
	| { type: 'catalog-scope'; name: string }
	| {
			type: 'catalog-query';
			name: string;
			query: CatalogQueryName;
			rows: number;
			durationMs: number;
			truncated: boolean;
	  }
	| { type: 'catalog-done'; objects: number; foreignKeys: number }
	| { type: 'catalog-warning'; code: 'truncated' | 'empty'; name: string }
	| { type: 'comments-skip' }
	| { type: 'inference-skip' }
	| { type: 'comments-batch'; current: number; total: number; count: number }
	| {
			type: 'comments-batch-done';
			current: number;
			total: number;
			accepted: number;
			durationMs: number;
	  }
	| {
			type: 'comments-batch-failed';
			current: number;
			total: number;
			count: number;
			durationMs: number;
			ids: string[];
			promptChars: number;
			detail: string;
	  }
	| { type: 'relations-start'; objects: number; known: number }
	| { type: 'relations-done'; returned: number; kept: number; durationMs: number }
	| { type: 'relations-failed'; durationMs: number; detail: string };

export function analysisLogStage(event: AnalysisLogEvent): AnalysisStage {
	switch (event.type) {
		case 'catalog-scope':
		case 'catalog-query':
		case 'catalog-done':
		case 'catalog-warning':
			return 'catalog';
		case 'comments-skip':
		case 'comments-batch':
		case 'comments-batch-done':
		case 'comments-batch-failed':
			return 'comments';
		case 'relations-start':
		case 'relations-done':
		case 'relations-failed':
			return 'relations';
		case 'inference-skip':
			return 'done';
	}
}

export function analysisLogLevel(event: AnalysisLogEvent): AnalysisLogLevel {
	if (event.type === 'comments-batch-failed' || event.type === 'relations-failed') return 'error';
	if (event.type === 'catalog-warning' || event.type === 'inference-skip') return 'warning';
	if (event.type === 'catalog-query' && event.truncated) return 'warning';
	if (
		event.type === 'catalog-done' ||
		event.type === 'comments-batch-done' ||
		event.type === 'relations-done'
	) {
		return 'success';
	}
	return 'info';
}

export function formatAnalysisError(error: unknown): string {
	const parts: string[] = [];
	const seen = new Set<string>();
	const objects = new WeakSet<object>();
	const push = (text: string) => {
		const trimmed = text.trim().slice(0, 800);
		if (!trimmed || seen.has(trimmed)) return;
		seen.add(trimmed);
		parts.push(trimmed);
	};
	walkError(error, push, objects, 0);
	return parts.join('\n') || 'Unexpected error';
}

function walkError(
	error: unknown,
	push: (text: string) => void,
	objects: WeakSet<object>,
	depth: number
) {
	if (depth > 8 || error == null) return;
	if (typeof error === 'string' || typeof error === 'number' || typeof error === 'boolean') {
		push(String(error));
		return;
	}
	if (typeof error !== 'object') return;
	if (objects.has(error)) return;
	objects.add(error);
	const record = error as Record<string, unknown>;
	if (typeof record.name === 'string' && record.name !== 'Error' && record.name !== 'Object') {
		push(record.name);
	}
	if (typeof record.message === 'string') push(record.message);
	if (typeof record.status === 'number') push(`HTTP ${record.status}`);
	else if (typeof record.statusCode === 'number') push(`HTTP ${record.statusCode}`);
	if (typeof record.code === 'string' || typeof record.code === 'number') push(String(record.code));
	if (typeof record.type === 'string') push(record.type);
	if (typeof record.param === 'string' && record.param) push(`param ${record.param}`);
	if (typeof record.lc_error_code === 'string') push(record.lc_error_code);
	const request = requestId(record);
	if (request) push(`request-id ${request}`);
	const output = snippet(record.llmOutput ?? record.observation);
	if (output) push(`output: ${output}`);
	if (record.response && record.response !== error) extractResponse(record.response, push);
	if (record.error !== error) walkError(record.error, push, objects, depth + 1);
	if (record.cause !== error) walkError(record.cause, push, objects, depth + 1);
}

function extractResponse(response: unknown, push: (text: string) => void) {
	if (!response || typeof response !== 'object') {
		const text = snippet(response);
		if (text) push(text);
		return;
	}
	const record = response as Record<string, unknown>;
	if (typeof record.status === 'number') push(`HTTP ${record.status}`);
	if (typeof record.statusText === 'string' && record.statusText.trim())
		push(record.statusText.trim());
	const request = requestId(record);
	if (request) push(`request-id ${request}`);
	const body = snippet(record.data ?? record.body ?? record.text);
	if (body) push(body);
}

function requestId(record: Record<string, unknown>): string | null {
	if (typeof record.request_id === 'string' && record.request_id.trim())
		return record.request_id.trim();
	if (typeof record.requestId === 'string' && record.requestId.trim())
		return record.requestId.trim();
	return headerValue(record.headers, 'x-request-id') ?? headerValue(record.headers, 'request-id');
}

function headerValue(headers: unknown, name: string): string | null {
	if (!headers || typeof headers !== 'object') return null;
	if (typeof (headers as { get?: unknown }).get === 'function') {
		const value = (headers as { get: (header: string) => string | null }).get(name);
		return value?.trim() || null;
	}
	const record = headers as Record<string, unknown>;
	const value = record[name] ?? record[name.toLowerCase()];
	return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function snippet(value: unknown): string | null {
	if (typeof value === 'string') {
		const trimmed = value.trim();
		return trimmed ? trimmed.slice(0, 500) : null;
	}
	if (!value || typeof value !== 'object') return null;
	try {
		const text = JSON.stringify(value);
		return text && text !== '{}' ? text.slice(0, 500) : null;
	} catch {
		return null;
	}
}
