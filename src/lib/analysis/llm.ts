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

export function createAnalysisCaller(settings: LlmSettings): StructuredCaller {
	const model = settings.model.trim() || DEFAULT_MODELS[settings.provider];
	const apiKey = settings.apiKey.trim();
	const baseUrl = settings.baseUrl.trim();
	return {
		async complete<T>(schema: ZodType<T>, system: string, human: string): Promise<T> {
			try {
				const raw = await api.completeLlm({
					provider: settings.provider,
					apiKey,
					model,
					baseUrl,
					system,
					human,
					schema: analysisResponseSchema(schema)
				});
				return schema.parse(raw);
			} catch (caught) {
				throw normalizeInvokeError(caught);
			}
		}
	};
}
