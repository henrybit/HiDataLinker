import { ChatAnthropic } from '@langchain/anthropic';
import { ChatOpenAI } from '@langchain/openai';
import type { ZodType } from 'zod';
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

export function createAnalysisCaller(settings: LlmSettings): StructuredCaller {
	const model = settings.model.trim() || DEFAULT_MODELS[settings.provider];
	const apiKey = settings.apiKey.trim();
	const baseUrl = settings.baseUrl.trim();
	const chat =
		settings.provider === 'anthropic'
			? new ChatAnthropic({
					model,
					apiKey,
					temperature: 0,
					anthropicApiUrl: baseUrl || undefined
				})
			: new ChatOpenAI({
					model,
					apiKey,
					temperature: 0,
					configuration: baseUrl ? { baseURL: baseUrl } : undefined
				});
	return {
		async complete<T>(schema: ZodType<T>, system: string, human: string): Promise<T> {
			const structured = chat.withStructuredOutput(schema);
			const result = await structured.invoke([
				{ role: 'system', content: system },
				{ role: 'user', content: human }
			]);
			return result as T;
		}
	};
}
