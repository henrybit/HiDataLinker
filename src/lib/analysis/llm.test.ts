import { describe, expect, it } from 'vitest';
import { commentResponseSchema } from './prompts';
import {
	analysisResponseSchema,
	LLM_POLL_INTERVAL_MS,
	normalizeInvokeError,
	waitForLlmJob
} from './llm';

describe('analysisResponseSchema', () => {
	it('drops draft metadata and keeps the comment object schema', () => {
		const schema = analysisResponseSchema(commentResponseSchema);
		expect(schema.$schema).toBeUndefined();
		expect(schema.type).toBe('object');
		const properties = schema.properties as { comments: { type: string } };
		expect(properties.comments.type).toBe('array');
	});
});

describe('normalizeInvokeError', () => {
	it('turns a string rejection into an Error that can take extra fields', () => {
		const error = normalizeInvokeError('Command complete_llm not allowed by ACL');
		expect(error).toBeInstanceOf(Error);
		expect(error.message).toBe('Command complete_llm not allowed by ACL');
		expect(() => {
			(error as Error & { pregelTaskId?: string }).pregelTaskId = 'task';
		}).not.toThrow();
	});
});

describe('waitForLlmJob', () => {
	it('waits between polls and returns the finished payload', async () => {
		const reads = [
			{ status: 'pending' },
			{ status: 'pending' },
			{ status: 'done', result: { relations: [] } }
		];
		const waits: number[] = [];
		const result = await waitForLlmJob(
			async () => reads.shift() ?? { status: 'failed', error: 'missing' },
			async (ms) => {
				waits.push(ms);
			}
		);
		expect(result).toEqual({ relations: [] });
		expect(waits).toEqual([LLM_POLL_INTERVAL_MS, LLM_POLL_INTERVAL_MS]);
	});

	it('surfaces a failed job without treating it as a successful empty result', async () => {
		await expect(
			waitForLlmJob(async () => ({
				status: 'failed',
				error: 'model request timed out\noperation timed out'
			}))
		).rejects.toThrow('model request timed out');
	});
});
