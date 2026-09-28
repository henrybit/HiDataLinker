import { DEFAULT_MODELS, type LlmSettings } from './llm';

const STORAGE_KEY = 'db-gui-relationship-llm';

export function defaultLlmSettings(): LlmSettings {
	return {
		provider: 'openai',
		apiKey: '',
		model: DEFAULT_MODELS.openai,
		baseUrl: ''
	};
}

export function loadLlmSettings(): LlmSettings {
	const defaults = defaultLlmSettings();
	if (typeof localStorage === 'undefined') return defaults;
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		if (!raw) return defaults;
		const parsed = JSON.parse(raw) as Partial<LlmSettings>;
		const provider = parsed.provider === 'anthropic' ? 'anthropic' : 'openai';
		return {
			provider,
			apiKey: typeof parsed.apiKey === 'string' ? parsed.apiKey : '',
			model:
				typeof parsed.model === 'string' && parsed.model.trim()
					? parsed.model
					: DEFAULT_MODELS[provider],
			baseUrl: typeof parsed.baseUrl === 'string' ? parsed.baseUrl : ''
		};
	} catch {
		return defaults;
	}
}

export function saveLlmSettings(settings: LlmSettings) {
	if (typeof localStorage === 'undefined') return;
	localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}
