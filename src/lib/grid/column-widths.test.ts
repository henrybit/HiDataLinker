import { describe, expect, it } from 'vitest';
import {
	DATA_GRID_MAX_COL_WIDTH,
	DATA_GRID_MIN_COL_WIDTH,
	cellNeedsExpand,
	dataGridLayout,
	type GridTextKind
} from './column-widths';

function pxPerChar(text: string, _kind: GridTextKind): number {
	return text.length * 10;
}

describe('data grid column widths', () => {
	it('sizes each column from its own header and cells', () => {
		const layout = dataGridLayout(
			[{ name: 'id' }, { name: 'remark' }],
			[
				['1', null],
				['2', 'short']
			],
			'(NULL)',
			pxPerChar
		);

		expect(layout.columnWidths[0]).toBeLessThan(layout.columnWidths[1]);
		expect(layout.tableWidth).toBe(
			layout.indexWidth + layout.columnWidths.reduce((sum, width) => sum + width, 0)
		);
	});

	it('does not change widths when row order changes', () => {
		const columns = [{ name: 'ruleConfig' }];
		const withLongFirst = dataGridLayout(
			columns,
			[['{"a":1}'], [null], ['x']],
			'(NULL)',
			pxPerChar
		);
		const withLongLast = dataGridLayout(columns, [[null], ['x'], ['{"a":1}']], '(NULL)', pxPerChar);
		expect(withLongLast.columnWidths).toEqual(withLongFirst.columnWidths);
	});

	it('caps long text at the cell max width', () => {
		const layout = dataGridLayout(
			[{ name: 'riskDescription' }],
			[['中'.repeat(200)]],
			'(NULL)',
			pxPerChar
		);
		expect(layout.columnWidths[0]).toBe(DATA_GRID_MAX_COL_WIDTH);
	});

	it('keeps a floor for narrow columns', () => {
		const layout = dataGridLayout([{ name: 'a' }], [['b']], '(NULL)', () => 1);
		expect(layout.columnWidths[0]).toBe(DATA_GRID_MIN_COL_WIDTH);
		expect(layout.indexWidth).toBeGreaterThanOrEqual(48);
	});

	it('treats empty cells like null and widens the index for large pages', () => {
		const layout = dataGridLayout(
			[{ name: 'priority' }],
			Array.from({ length: 5000 }, () => ['']),
			'(NULL)',
			pxPerChar
		);
		expect(layout.indexWidth).toBeGreaterThan(48);
		expect(layout.columnWidths[0]).toBeGreaterThan(DATA_GRID_MIN_COL_WIDTH);
	});
});

describe('cellNeedsExpand', () => {
	it('flags long or multiline values', () => {
		expect(cellNeedsExpand('x'.repeat(47))).toBe(false);
		expect(cellNeedsExpand('x'.repeat(48))).toBe(true);
		expect(cellNeedsExpand('a\nb')).toBe(true);
	});
});
