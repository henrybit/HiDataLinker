import { createCipheriv, randomBytes } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import type { QueryResult } from '$lib/api/types';
import { applyFieldCodecs, newFieldCodecRule, type FieldCodecRule } from './field-codec';

function columns(names: string[]): QueryResult['columns'] {
	return names.map((name) => ({ name, typeName: 'text' }));
}

function rule(
	partial: Omit<Partial<FieldCodecRule>, 'aes'> &
		Pick<FieldCodecRule, 'op' | 'column'> & { aes?: Partial<FieldCodecRule['aes']> }
): FieldCodecRule {
	const base = newFieldCodecRule(partial.id ?? partial.column, partial.column);
	return {
		...base,
		...partial,
		enabled: partial.enabled ?? true,
		aes: { ...base.aes, ...partial.aes }
	};
}

function encrypt(
	algorithm: string,
	key: Buffer,
	iv: Buffer | null,
	plain: string
): { cipher: Buffer; tag?: Buffer } {
	const cipher = createCipheriv(algorithm, key, iv);
	const body = Buffer.concat([cipher.update(plain, 'utf8'), cipher.final()]);
	if (!algorithm.includes('gcm')) return { cipher: body };
	return { cipher: body, tag: (cipher as unknown as { getAuthTag(): Buffer }).getAuthTag() };
}

describe('applyFieldCodecs', () => {
	it('decodes base64, hex and URL text and leaves failures unchanged', async () => {
		const applied = await applyFieldCodecs(
			columns(['name', 'code', 'path', 'raw']),
			[
				['aGVsbG8=', '6869', '%E4%B8%AD', null],
				['@@@', 'zz', '%', '']
			],
			[
				rule({ column: 'Name', op: 'base64' }),
				rule({ column: 'code', op: 'hex' }),
				rule({ column: 'path', op: 'uri' }),
				rule({ column: 'missing', op: 'base64' })
			]
		);
		expect(applied.rows).toEqual([
			['hello', 'hi', '中', null],
			['@@@', 'zz', '%', '']
		]);
		expect(applied.failedCells).toBe(3);
		expect(applied.activeColumns).toEqual([0, 1, 2]);
		expect(applied.issues).toEqual([{ id: 'missing', column: 'missing', issue: 'unknown-column' }]);
	});

	it('leaves the cell unchanged until the rule is confirmed', async () => {
		const applied = await applyFieldCodecs(
			columns(['name']),
			[['aGVsbG8=']],
			[rule({ column: 'name', op: 'base64', enabled: false })]
		);
		expect(applied.rows).toEqual([['aGVsbG8=']]);
		expect(applied.activeColumns).toEqual([]);
		expect(applied.issues).toEqual([]);
	});

	it('runs several rules on one column from top to bottom', async () => {
		const applied = await applyFieldCodecs(
			columns(['v']),
			[[encodeURIComponent(btoa('hi'))]],
			[rule({ column: 'v', op: 'uri' }), rule({ id: 'b64', column: 'v', op: 'base64' })]
		);
		expect(applied.rows).toEqual([['hi']]);
	});

	it('decrypts AES-ECB, CBC and GCM fields stored as base64', async () => {
		const key = Buffer.from('0123456789abcdef');
		const iv = Buffer.from('abcdef0123456789');
		const gcmIv = randomBytes(12);
		const ecb = encrypt('aes-128-ecb', key, null, 'ecb-明文');
		const cbc = encrypt('aes-128-cbc', key, iv, 'cbc-明文');
		const gcm = encrypt('aes-128-gcm', key, gcmIv, 'gcm-明文');
		const gcmPacked = Buffer.concat([gcm.cipher, gcm.tag!]);
		const cbcPacked = Buffer.concat([iv, cbc.cipher]);

		const applied = await applyFieldCodecs(
			columns(['ecb', 'cbc', 'gcm', 'prefixed']),
			[
				[
					ecb.cipher.toString('base64'),
					cbc.cipher.toString('base64'),
					gcmPacked.toString('base64'),
					cbcPacked.toString('base64')
				]
			],
			[
				rule({
					column: 'ecb',
					op: 'aes',
					aes: { key: '0123456789abcdef', mode: 'ECB', input: 'base64' }
				}),
				rule({
					column: 'cbc',
					op: 'aes',
					aes: { key: '0123456789abcdef', mode: 'CBC', iv: 'abcdef0123456789', input: 'base64' }
				}),
				rule({
					id: 'gcm',
					column: 'gcm',
					op: 'aes',
					aes: {
						key: key.toString('hex'),
						keyEncoding: 'hex',
						mode: 'GCM',
						iv: gcmIv.toString('hex'),
						ivEncoding: 'hex',
						input: 'base64'
					}
				}),
				rule({
					column: 'prefixed',
					op: 'aes',
					aes: { key: '0123456789abcdef', mode: 'CBC', ivInCipher: true, input: 'base64' }
				})
			]
		);

		const gcmRule = applied.issues.find((issue) => issue.id === 'gcm');
		expect(gcmRule).toBeUndefined();
		expect(applied.rows[0]).toEqual(['ecb-明文', 'cbc-明文', 'gcm-明文', 'cbc-明文']);
	});

	it('decrypts AES-256-CBC when the IV is sixteen zero bytes', async () => {
		const key = randomBytes(32);
		const encrypted = encrypt('aes-256-cbc', key, Buffer.alloc(16), 'zero-iv');
		const applied = await applyFieldCodecs(
			columns(['secret']),
			[[encrypted.cipher.toString('hex')]],
			[
				rule({
					column: 'secret',
					op: 'aes',
					aes: {
						key: key.toString('base64'),
						keyEncoding: 'base64',
						mode: 'CBC',
						input: 'hex'
					}
				})
			]
		);
		expect(applied.issues).toEqual([]);
		expect(applied.rows).toEqual([['zero-iv']]);
	});

	it('reports a key that is not 16, 24 or 32 bytes and skips the column', async () => {
		const applied = await applyFieldCodecs(
			columns(['secret']),
			[['aGVsbG8=']],
			[rule({ column: 'secret', op: 'aes', aes: { key: 'short', mode: 'ECB' } })]
		);
		expect(applied.rows).toEqual([['aGVsbG8=']]);
		expect(applied.issues).toEqual([{ id: 'secret', column: 'secret', issue: 'bad-key' }]);
	});
});
