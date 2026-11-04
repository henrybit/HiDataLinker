import { describe, expect, it } from 'vitest';
import {
	detectModelModality,
	normalizeSelectableModality,
	resolveProfileModality
} from './modality';

describe('detectModelModality', () => {
	it('marks common vision / omni models as multimodal', () => {
		expect(detectModelModality('gpt-4o-mini')).toBe('multimodal');
		expect(detectModelModality('gpt-4.1')).toBe('multimodal');
		expect(detectModelModality('claude-sonnet-4-5')).toBe('multimodal');
		expect(detectModelModality('claude-3-5-sonnet')).toBe('multimodal');
		expect(detectModelModality('gemini-2.0-flash')).toBe('multimodal');
		expect(detectModelModality('qwen2.5-vl-72b')).toBe('multimodal');
		expect(detectModelModality('o3-mini')).toBe('multimodal');
	});

	it('marks classic text models as text', () => {
		expect(detectModelModality('gpt-3.5-turbo')).toBe('text');
		expect(detectModelModality('gpt-4-0613')).toBe('text');
		expect(detectModelModality('deepseek-chat')).toBe('text');
		expect(detectModelModality('qwen-plus')).toBe('text');
		expect(detectModelModality('text-embedding-3-large')).toBe('text');
	});

	it('returns unknown for blank or unfamiliar ids', () => {
		expect(detectModelModality('')).toBe('unknown');
		expect(detectModelModality('my-custom-proxy-model')).toBe('unknown');
	});
});

describe('resolveProfileModality', () => {
	it('prefers the explicit profile modality', () => {
		expect(resolveProfileModality({ model: 'gpt-4o-mini', modality: 'text' })).toBe('text');
		expect(resolveProfileModality({ model: 'deepseek-chat', modality: 'multimodal' })).toBe(
			'multimodal'
		);
	});

	it('falls back to model-name detection', () => {
		expect(resolveProfileModality({ model: 'gpt-4o-mini' })).toBe('multimodal');
		expect(resolveProfileModality({ model: 'deepseek-chat' })).toBe('text');
	});
});

describe('normalizeSelectableModality', () => {
	it('keeps valid selections and maps missing values from the model id', () => {
		expect(normalizeSelectableModality('text')).toBe('text');
		expect(normalizeSelectableModality('multimodal')).toBe('multimodal');
		expect(normalizeSelectableModality(undefined, 'gpt-4o-mini')).toBe('multimodal');
		expect(normalizeSelectableModality('nope', 'deepseek-chat')).toBe('text');
		expect(normalizeSelectableModality(undefined, 'custom-model')).toBe('text');
	});
});
