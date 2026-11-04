/** Best-effort classification of chat models for UI labeling. */

export type ModelModality = 'text' | 'multimodal' | 'unknown';

/** User-selectable modalities when configuring a provider. */
export type SelectableModelModality = 'text' | 'multimodal';

/**
 * Prefer an explicit profile modality; otherwise infer from the model id.
 */
export function resolveProfileModality(profile: {
	model: string;
	modality?: ModelModality | null;
}): ModelModality {
	if (profile.modality === 'text' || profile.modality === 'multimodal') {
		return profile.modality;
	}
	if (profile.modality === 'unknown') return 'unknown';
	return detectModelModality(profile.model);
}

export function normalizeSelectableModality(
	value: unknown,
	fallbackModel = ''
): SelectableModelModality {
	if (value === 'text' || value === 'multimodal') return value;
	const detected = detectModelModality(fallbackModel);
	return detected === 'multimodal' ? 'multimodal' : 'text';
}

/**
 * Infer whether a model id is text-only or multimodal (vision / audio / etc.).
 * Heuristic on common OpenAI / Anthropic / Gemini / open-source names — not a guarantee.
 */
export function detectModelModality(model: string): ModelModality {
	const m = model.trim().toLowerCase();
	if (!m) return 'unknown';

	// Explicit multimodal families / markers.
	if (
		/(^|[^\w])(vision|vl|vllm|multimodal|omni|pixtral|llava|internvl|qwen[-_.]?vl|qwen2[-_.]?vl|qwen2\.5[-_.]?vl|gpt-4o|gpt-4\.1|gpt-5|chatgpt-4o|o[1-9](-|$)|claude-3|claude-4|claude-sonnet-4|claude-opus-4|claude-haiku-4|gemini|gemma-3|llama[-_.]?4|mistral[-_.]?small[-_.]?3|phi-4[-_.]?multimodal)/.test(
			m
		) ||
		/gpt-4-turbo|gpt-4\.turbo|gpt-4-vision/.test(m)
	) {
		return 'multimodal';
	}

	// Explicit text-only / non-chat multimodal.
	if (
		/(embedding|text-embedding|whisper|tts|davinci|curie|babbage|ada|gpt-3\.5|instruct|deepseek-coder|deepseek-r1|deepseek-chat|codestral|codellama|starcoder|qwen[-_.]?(turbo|plus|max|long)|llama-3(\.\d)?(?!.*vision)|mistral-(tiny|small|medium|large)(?!.*vision)|claude-2|claude-instant)/.test(
			m
		)
	) {
		return 'text';
	}

	// Classic GPT-4 snapshot without vision suffix → treat as text.
	if (/^gpt-4($|-\d{4})/.test(m)) return 'text';

	return 'unknown';
}

export function modalityI18nKey(modality: ModelModality): string {
	switch (modality) {
		case 'multimodal':
			return 'agentMode.composer.modalityMultimodal';
		case 'text':
			return 'agentMode.composer.modalityText';
		default:
			return 'agentMode.composer.modalityUnknown';
	}
}
