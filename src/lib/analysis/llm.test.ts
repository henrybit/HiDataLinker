import { describe, expect, it } from 'vitest';
import { commentResponseSchema } from './prompts';
import { analysisResponseSchema, normalizeInvokeError } from './llm';

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
