import { describe, expect, it } from 'vitest';
import { combineDocuments, DocumentReadError, extractDocumentText } from './documents';
import { edgeVisible, layoutNodes } from './layout';
import type { RelationshipEdge } from './types';

describe('documents', () => {
	it('reads markdown and rejects unsupported word and other files', async () => {
		const text = await extractDocumentText(
			'notes.md',
			new TextEncoder().encode('# Orders\nuser_id references users')
		);
		expect(text).toContain('references users');
		await expect(extractDocumentText('legacy.doc', new Uint8Array([1, 2]))).rejects.toMatchObject({
			code: 'doc',
			fileName: 'legacy.doc'
		} satisfies Partial<DocumentReadError>);
		await expect(extractDocumentText('image.png', new Uint8Array([1]))).rejects.toMatchObject({
			code: 'unsupported'
		});
		expect(combineDocuments([{ name: 'a.md', text: 'hello' }])).toContain('# a.md');
	});
});

describe('layout', () => {
	it('places nodes from different schemas apart and filters edges', () => {
		const positions = layoutNodes(
			[
				{ id: 'a', group: 'shop' },
				{ id: 'b', group: 'shop' },
				{ id: 'c', group: 'billing' }
			],
			[{ fromId: 'a', toId: 'c' }]
		);
		expect(positions.a.x).not.toBe(positions.c.x);
		expect(positions.a.y).not.toBe(positions.b.y);
		const edge: RelationshipEdge = {
			id: 'e',
			fromId: 'a',
			toId: 'c',
			fromColumns: [],
			toColumns: [],
			origin: 'inferred',
			strength: 'weak',
			cardinality: 'many_to_one',
			reasonCode: 'inferred',
			reason: 'guess'
		};
		expect(edgeVisible(edge, { physical: true, inferred: true, strong: true, weak: true })).toBe(
			true
		);
		expect(edgeVisible(edge, { physical: true, inferred: false, strong: true, weak: true })).toBe(
			false
		);
		expect(edgeVisible(edge, { physical: true, inferred: true, strong: true, weak: false })).toBe(
			false
		);
	});
});
