import { z, type ZodType } from 'zod';
import { api } from '$lib/api/tauri';
import { formatAnalysisError } from './log';
import type { StructuredCaller } from './workflow';

export type LlmProvider = 'openai' | 'anthropic';

export interface LlmSettings {
	provider: LlmProvider;
	apiKey: string;
	model: string;
	baseUrl: string;
}

export const DEFAULT_MODELS: Record<LlmProvider, string> = {
	openai: 'gpt-4o-mini',
	anthropic: 'claude-sonnet-4-5'
};

/** JSON schema sent to the Rust caller. Draft metadata is stripped for provider APIs. */
export function analysisResponseSchema(schema: ZodType): Record<string, unknown> {
	const json = { ...(z.toJSONSchema(schema) as Record<string, unknown>) };
	delete json.$schema;
	return json;
}

/**
 * Tauri invoke rejects with a string. LangGraph then assigns `pregelTaskId` on
 * that value, which WebKit rejects as "Attempted to assign to readonly property."
 */
export function normalizeInvokeError(caught: unknown): Error {
	if (caught instanceof Error) return caught;
	if (typeof caught === 'string' && caught.trim()) return new Error(caught);
	return new Error(formatAnalysisError(caught));
}

export const LLM_POLL_INTERVAL_MS = 2_000;

export interface LlmJobStatus {
	status: 'pending' | 'done' | 'failed' | string;
	result?: unknown;
	error?: string | null;
}

/** Poll a background model job. The first check is immediate; later checks wait. */
export async function waitForLlmJob(
	read: () => Promise<LlmJobStatus>,
	wait: (ms: number) => Promise<void> = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
): Promise<unknown> {
	for (;;) {
		const job = await read();
		if (job.status === 'done') return job.result;
		if (job.status === 'failed') {
			throw new Error(job.error?.trim() || 'model request failed');
		}
		if (job.status !== 'pending') {
			throw new Error(job.error?.trim() || 'model request failed');
		}
		await wait(LLM_POLL_INTERVAL_MS);
	}
}

export function createAnalysisCaller(settings: LlmSettings): StructuredCaller {
	const model = settings.model.trim() || DEFAULT_MODELS[settings.provider];
	const apiKey = settings.apiKey.trim();
	const baseUrl = settings.baseUrl.trim();
	return {
		async complete<T>(schema: ZodType<T>, system: string, human: string): Promise<T> {
			try {
				const request = {
					provider: settings.provider,
					apiKey,
					model,
					baseUrl,
					system,
					human,
					schema: analysisResponseSchema(schema)
				};
				const id = await api.startLlm(request);
				const raw = await waitForLlmJob(() => api.pollLlm(id));
				return schema.parse(raw);
			} catch (caught) {
				throw normalizeInvokeError(caught);
			}
		}
	};
}
