import { describe, expect, it } from 'vitest';
import { parseChartSpec } from './chart-spec';
import { renderMarkdownToSafeHtml, sanitizeHtml, splitRichContent } from './rich-content';

describe('chart-spec', () => {
	it('parses bar chart JSON', () => {
		const spec = parseChartSpec(
			JSON.stringify({
				type: 'bar',
				title: 'Revenue',
				categories: ['A', 'B'],
				series: [{ name: 'QoQ', data: [1, -2] }]
			})
		);
		expect(spec?.type).toBe('bar');
		expect(spec?.categories).toEqual(['A', 'B']);
		expect(spec?.series?.[0]?.data).toEqual([1, -2]);
	});

	it('parses pie chart JSON', () => {
		const spec = parseChartSpec(
			JSON.stringify({
				type: 'pie',
				data: [
					{ name: 'Google', value: 12 },
					{ name: 'Other', value: 8 }
				]
			})
		);
		expect(spec?.type).toBe('pie');
		expect(spec?.data).toHaveLength(2);
	});
});

describe('rich-content', () => {
	it('splits markdown, html, and chart fences', () => {
		const segments = splitRichContent(
			'Hello\n\n```html\n<strong>Hi</strong>\n```\n\n```chart\n{"type":"pie","data":[{"name":"A","value":1}]}\n```\n\nTail'
		);
		expect(segments.map((item) => item.kind)).toEqual([
			'markdown',
			'html',
			'chart',
			'markdown'
		]);
		expect(segments[2]?.kind === 'chart' && segments[2].spec.type).toBe('pie');
	});

	it('renders markdown to safe html', () => {
		const html = renderMarkdownToSafeHtml('**bold** and [link](https://example.com)');
		expect(html).toContain('<strong>bold</strong>');
		expect(html).toContain('href="https://example.com"');
	});

	it('strips dangerous html', () => {
		const clean = sanitizeHtml('<img src=x onerror="alert(1)"><p>ok</p>');
		expect(clean).toContain('<p>ok</p>');
		expect(clean.toLowerCase()).not.toContain('onerror');
	});
});
