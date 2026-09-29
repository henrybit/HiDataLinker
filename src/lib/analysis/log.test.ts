import { describe, expect, it } from 'vitest';
import type { QueryResult } from '$lib/api/types';
import { loadSchemaCatalog } from './collect';
import { formatAnalysisError } from './log';
import type { SchemaScope } from './types';

describe('formatAnalysisError', () => {
	it('keeps the message and walks the cause chain', () => {
		const error = new Error('request failed', {
			cause: Object.assign(new Error('model rejected the schema'), {
				status: 400,
				code: 'invalid_request'
			})
		});
		expect(formatAnalysisError(error)).toBe(
			'request failed\nmodel rejected the schema\nHTTP 400\ninvalid_request'
		);
	});

	it('reads a plain object and does not repeat the same text', () => {
		expect(
			formatAnalysisError({
				message: 'upstream timeout',
				error: { message: 'upstream timeout', statusCode: 504 }
			})
		).toBe('upstream timeout\nHTTP 504');
	});

	it('includes the parser output, request id, and response body', () => {
		const error = Object.assign(new Error('Failed to parse'), {
			name: 'OutputParserException',
			status: 422,
			llmOutput: '{"comments":"bad"}',
			request_id: 'req_1',
			response: {
				status: 422,
				data: { error: { message: 'response_format is not supported' } }
			}
		});
		expect(formatAnalysisError(error)).toBe(
			[
				'OutputParserException',
				'Failed to parse',
				'HTTP 422',
				'request-id req_1',
				'output: {"comments":"bad"}',
				'{"error":{"message":"response_format is not supported"}}'
			].join('\n')
		);
	});

	it('falls back when the thrown value has no message', () => {
		expect(formatAnalysisError({ code: 0 })).toBe('0');
		expect(formatAnalysisError(undefined)).toBe('Unexpected error');
	});
});

describe('loadSchemaCatalog logs', () => {
	it('records each catalog query and a truncated warning', async () => {
		const events: string[] = [];
		const scope: SchemaScope = {
			connectionId: 'c1',
			connectionName: 'App',
			engine: 'mysql',
			schema: 'shop'
		};
		await loadSchemaCatalog(
			[scope],
			async () => emptyResult(true),
			(event) => events.push(event.type === 'catalog-query' ? event.query : event.type)
		);
		expect(events).toEqual([
			'catalog-scope',
			'objects',
			'columns',
			'foreignKeys',
			'views',
			'catalog-done',
			'catalog-warning',
			'catalog-warning'
		]);
	});
});

function emptyResult(truncated: boolean): QueryResult {
	return {
		columns: [{ name: 'object_name', typeName: 'text' }],
		rows: [],
		affectedRows: 0,
		durationMs: 1,
		truncated,
		statementKind: 'query'
	};
}
