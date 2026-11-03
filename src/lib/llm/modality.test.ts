import { describe, expect, it } from 'vitest';
import { detectModelModality } from './modality';

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
