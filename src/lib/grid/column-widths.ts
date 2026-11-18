/** Matches `.data-grid` cell `max-width`. */
export const DATA_GRID_MAX_COL_WIDTH = 420;
export const DATA_GRID_MIN_COL_WIDTH = 56;
export const DATA_GRID_INDEX_MIN_WIDTH = 48;
/** Show the expand control once content is likely clipped. */
export const EXPAND_MIN_CHARS = 48;

const CELL_CHROME = 8 + 8 + 1 + 2;
const EXPAND_BUTTON_WIDTH = 26;
const SAMPLE_CHARS = 80;

export type GridTextKind = 'header' | 'cell' | 'null';

export function cellNeedsExpand(value: string): boolean {
	return value.length >= EXPAND_MIN_CHARS || /[\r\n]/.test(value);
}

/** Fallback when canvas measurement is unavailable. Widths are slightly generous. */
export function estimateTextWidth(text: string, kind: GridTextKind): number {
	const bold = kind === 'header';
	const italic = kind === 'null';
	let width = 0;
	for (const char of text) {
		const code = char.codePointAt(0) ?? 0;
		let charWidth = code <= 0xff ? 7.4 : 13;
		if (bold) charWidth *= 1.08;
		if (italic) charWidth *= 1.05;
		width += charWidth;
		if (width >= DATA_GRID_MAX_COL_WIDTH) return DATA_GRID_MAX_COL_WIDTH;
	}
	return width;
}

export interface DataGridLayout {
	indexWidth: number;
	columnWidths: number[];
	tableWidth: number;
}

/**
 * Column widths for the full result, not the virtual window.
 * Auto table layout remeasures whatever rows are mounted, so scrolling a
 * windowed grid on Windows (where the scrollbar also changes the viewport)
 * makes variable-width columns shake.
 */
export function dataGridLayout(
	columns: ReadonlyArray<{ name: string }>,
	rows: ReadonlyArray<ReadonlyArray<string | null | undefined>>,
	nullLabel = '(NULL)',
	measure?: (text: string, kind: GridTextKind) => number
): DataGridLayout {
	const textWidth = measure ?? estimateTextWidth;
	const columnWidths = columns.map((column, columnIndex) => {
		let content = textWidth(column.name, 'header');
		for (const row of rows) {
			const cell = row[columnIndex];
			if (cell == null || cell === '') {
				content = Math.max(content, textWidth(nullLabel, 'null'));
			} else {
				const sample = cell.length > SAMPLE_CHARS ? cell.slice(0, SAMPLE_CHARS) : cell;
				let width = textWidth(sample, 'cell');
				if (cellNeedsExpand(cell)) width += EXPAND_BUTTON_WIDTH;
				content = Math.max(content, width);
			}
			if (content >= DATA_GRID_MAX_COL_WIDTH - CELL_CHROME) break;
		}
		return clampColumnWidth(content);
	});

	const indexLabel = String(Math.max(rows.length, 1));
	const indexWidth = Math.max(
		DATA_GRID_INDEX_MIN_WIDTH,
		clampColumnWidth(Math.max(textWidth('#', 'header'), textWidth(indexLabel, 'cell')))
	);
	const tableWidth = indexWidth + columnWidths.reduce((sum, width) => sum + width, 0);
	return { indexWidth, columnWidths, tableWidth };
}

function clampColumnWidth(contentWidth: number): number {
	return Math.min(
		DATA_GRID_MAX_COL_WIDTH,
		Math.max(DATA_GRID_MIN_COL_WIDTH, Math.ceil(contentWidth + CELL_CHROME))
	);
}

/** Measure with the app font so locked widths match what the grid actually paints. */
export function browserTextMeasure(): ((text: string, kind: GridTextKind) => number) | undefined {
	if (typeof document === 'undefined') return undefined;
	const canvas = document.createElement('canvas');
	const context = canvas.getContext('2d');
	if (!context) return undefined;
	const family = getComputedStyle(document.body).fontFamily || 'sans-serif';
	return (text, kind) => {
		context.font =
			kind === 'header'
				? `600 13px ${family}`
				: kind === 'null'
					? `italic 400 13px ${family}`
					: `400 13px ${family}`;
		return context.measureText(text).width;
	};
}
