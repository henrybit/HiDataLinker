import type { QueryResult } from '$lib/api/types';
import { aesDecrypt } from './aes';

export type FieldCodecOp = 'base64' | 'hex' | 'uri' | 'aes';
export type ByteEncoding = 'utf8' | 'hex' | 'base64';
export type CipherEncoding = 'base64' | 'hex';
export type AesMode = 'ECB' | 'CBC' | 'GCM';

export interface AesCodecOptions {
	key: string;
	keyEncoding: ByteEncoding;
	mode: AesMode;
	iv: string;
	ivEncoding: ByteEncoding;
	input: CipherEncoding;
	/** CBC uses the first 16 bytes of the ciphertext; GCM uses the first 12. */
	ivInCipher: boolean;
}

export interface FieldCodecRule {
	id: string;
	column: string;
	op: FieldCodecOp;
	/** Applied to the result only after the rule is confirmed. */
	enabled: boolean;
	aes: AesCodecOptions;
}

export type CodecRuleIssue =
	'unknown-column' | 'missing-key' | 'bad-key' | 'bad-iv' | 'bad-material';

export interface CodecRuleProblem {
	id: string;
	column: string;
	issue: CodecRuleIssue;
}

export interface CodecApplyResult {
	rows: Array<Array<string | null>>;
	/** Columns a valid rule attempted to transform. */
	activeColumns: number[];
	failedCells: number;
	issues: CodecRuleProblem[];
}

export function emptyAesOptions(): AesCodecOptions {
	return {
		key: '',
		keyEncoding: 'utf8',
		mode: 'ECB',
		iv: '',
		ivEncoding: 'utf8',
		input: 'base64',
		ivInCipher: false
	};
}

export function newFieldCodecRule(id: string, column = ''): FieldCodecRule {
	return { id, column, op: 'base64', enabled: false, aes: emptyAesOptions() };
}

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder('utf-8', { fatal: true });

/** Copy into a fresh ArrayBuffer so WebCrypto accepts the view. */
function bytesOf(bytes: Uint8Array): Uint8Array<ArrayBuffer> {
	const copy = new ArrayBuffer(bytes.byteLength);
	const view = new Uint8Array(copy);
	view.set(bytes);
	return view;
}

function decodeBase64(value: string): Uint8Array {
	const compact = value.replace(/\s+/g, '').replace(/-/g, '+').replace(/_/g, '/');
	if (!compact || compact.length % 4 === 1) throw new Error('base64');
	const padded = compact + '='.repeat((4 - (compact.length % 4)) % 4);
	if (!/^[A-Za-z0-9+/]*={0,2}$/.test(padded)) throw new Error('base64');
	const binary = atob(padded);
	const bytes = new Uint8Array(binary.length);
	for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
	return bytes;
}

function decodeHex(value: string): Uint8Array {
	const compact = value.trim().replace(/^0x/i, '').replace(/\s+/g, '');
	if (!compact || compact.length % 2 !== 0 || /[^0-9a-f]/i.test(compact)) throw new Error('hex');
	const bytes = new Uint8Array(compact.length / 2);
	for (let index = 0; index < bytes.length; index += 1) {
		bytes[index] = Number.parseInt(compact.slice(index * 2, index * 2 + 2), 16);
	}
	return bytes;
}

function material(value: string, encoding: ByteEncoding): Uint8Array {
	if (encoding === 'utf8') return textEncoder.encode(value);
	if (encoding === 'hex') return decodeHex(value);
	return decodeBase64(value);
}

function bytesToText(bytes: Uint8Array): string {
	return textDecoder.decode(bytes);
}

function columnIndexes(columns: ReadonlyArray<{ name: string }>, name: string): number[] {
	const wanted = name.trim();
	if (!wanted) return [];
	const exact: number[] = [];
	const folded: number[] = [];
	const lower = wanted.toLowerCase();
	columns.forEach((column, index) => {
		if (column.name === wanted) exact.push(index);
		else if (column.name.toLowerCase() === lower) folded.push(index);
	});
	return exact.length ? exact : folded;
}

interface PreparedAes {
	mode: AesMode;
	key: Uint8Array;
	iv: Uint8Array | null;
	input: CipherEncoding;
	ivInCipher: boolean;
	gcmKey?: CryptoKey;
}

function prepareAes(options: AesCodecOptions): PreparedAes | CodecRuleIssue {
	if (!options.key) return 'missing-key';
	let key: Uint8Array;
	try {
		key = material(options.key, options.keyEncoding);
	} catch {
		return 'bad-material';
	}
	if (key.length !== 16 && key.length !== 24 && key.length !== 32) return 'bad-key';
	if (options.mode === 'ECB' || options.ivInCipher) {
		return {
			mode: options.mode,
			key,
			iv: null,
			input: options.input,
			ivInCipher: options.ivInCipher
		};
	}
	const expected = options.mode === 'GCM' ? 12 : 16;
	if (!options.iv) {
		if (options.mode === 'CBC') {
			return {
				mode: options.mode,
				key,
				iv: new Uint8Array(16),
				input: options.input,
				ivInCipher: false
			};
		}
		return 'bad-iv';
	}
	let iv: Uint8Array;
	try {
		iv = material(options.iv, options.ivEncoding);
	} catch {
		return 'bad-material';
	}
	if (iv.length !== expected) return 'bad-iv';
	return { mode: options.mode, key, iv, input: options.input, ivInCipher: false };
}

function decodeCipher(value: string, encoding: CipherEncoding): Uint8Array {
	return encoding === 'hex' ? decodeHex(value) : decodeBase64(value);
}

async function decryptAes(prepared: PreparedAes, value: string): Promise<string> {
	let data = decodeCipher(value, prepared.input);
	let iv = prepared.iv;
	if (prepared.ivInCipher) {
		const ivLength = prepared.mode === 'GCM' ? 12 : 16;
		if (data.length <= ivLength) throw new Error('aes-iv');
		iv = data.subarray(0, ivLength);
		data = data.subarray(ivLength);
	}
	let plain: Uint8Array;
	if (prepared.mode === 'GCM') {
		if (!iv || iv.length !== 12 || data.length < 16) throw new Error('aes-iv');
		if (!prepared.gcmKey) {
			prepared.gcmKey = await crypto.subtle.importKey(
				'raw',
				bytesOf(prepared.key),
				{ name: 'AES-GCM' },
				false,
				['decrypt']
			);
		}
		const decoded = await crypto.subtle.decrypt(
			{ name: 'AES-GCM', iv: bytesOf(iv), tagLength: 128 },
			prepared.gcmKey,
			bytesOf(data)
		);
		plain = new Uint8Array(decoded);
	} else {
		plain = aesDecrypt(prepared.mode, prepared.key, iv, data);
	}
	return bytesToText(plain);
}

function decodeText(op: Exclude<FieldCodecOp, 'aes'>, value: string): string {
	if (op === 'uri') return decodeURIComponent(value);
	const bytes = op === 'hex' ? decodeHex(value) : decodeBase64(value);
	return bytesToText(bytes);
}

export async function applyFieldCodecs(
	columns: QueryResult['columns'],
	rows: QueryResult['rows'],
	rules: readonly FieldCodecRule[]
): Promise<CodecApplyResult> {
	const issues: CodecRuleProblem[] = [];
	const jobs: Array<{ indexes: number[]; rule: FieldCodecRule; aes: PreparedAes | null }> = [];
	for (const rule of rules) {
		if (!rule.enabled) continue;
		const indexes = columnIndexes(columns, rule.column);
		if (!rule.column.trim()) continue;
		if (!indexes.length) {
			issues.push({ id: rule.id, column: rule.column, issue: 'unknown-column' });
			continue;
		}
		if (rule.op === 'aes') {
			const prepared = prepareAes(rule.aes);
			if (typeof prepared === 'string') {
				issues.push({ id: rule.id, column: rule.column, issue: prepared });
				continue;
			}
			jobs.push({ indexes, rule, aes: prepared });
		} else {
			jobs.push({ indexes, rule, aes: null });
		}
	}
	if (!jobs.length) return { rows, activeColumns: [], failedCells: 0, issues };

	const next = rows.map((row) => row.slice());
	const active = new Set<number>();
	let failedCells = 0;
	for (const job of jobs) {
		for (const index of job.indexes) active.add(index);
		for (let rowIndex = 0; rowIndex < next.length; rowIndex += 1) {
			for (const columnIndex of job.indexes) {
				const cell = next[rowIndex][columnIndex];
				if (cell == null || cell === '') continue;
				try {
					next[rowIndex][columnIndex] = job.aes
						? await decryptAes(job.aes, cell)
						: decodeText(job.rule.op as Exclude<FieldCodecOp, 'aes'>, cell);
				} catch {
					failedCells += 1;
				}
			}
		}
	}
	return { rows: next, activeColumns: [...active], failedCells, issues };
}
