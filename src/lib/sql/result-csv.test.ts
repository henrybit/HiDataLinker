import { describe, expect, it } from 'vitest';
import { csvFileName, resultToCsv } from './result-csv';

describe('resultToCsv', () => {
	it('quotes only what needs quoting and writes NULL as empty', () => {
		const csv = resultToCsv({
			columns: [
				{ name: 'id', dataType: 'int' },
				{ name: 'note, text', dataType: 'text' }
			],
			rows: [
				['1', 'plain'],
				['2', 'say "hi"\nbye'],
				['3', null],
				['4', ' padded']
			]
		} as never);
		expect(csv).toBe(
			'\uFEFFid,"note, text"\r\n1,plain\r\n2,"say ""hi""\nbye"\r\n3,\r\n4," padded"\r\n'
		);
	});

	it('keeps CJK text and builds safe file names', () => {
		expect(resultToCsv({ columns: [{ name: '客户' }], rows: [['张三']] } as never)).toBe(
			'\uFEFF客户\r\n张三\r\n'
		);
		expect(csvFileName('shop orders/2026')).toBe('shop_orders_2026.csv');
		expect(csvFileName('  ')).toBe('result.csv');
	});
});
