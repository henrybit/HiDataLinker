import DOMPurify from 'isomorphic-dompurify';
import { marked } from 'marked';
import {
	parseChartSpec,
	type AgentChartSpec,
	type ContentSegment
} from './chart-spec';

export type RichSegment =
	| { kind: 'markdown'; content: string }
	| { kind: 'html'; content: string }
	| { kind: 'chart'; spec: AgentChartSpec; raw: string };

const FENCE =
	/```(chart|html|htm)\s*\r?\n([\s\S]*?)```/gi;

marked.setOptions({
	gfm: true,
	breaks: true,
	pedantic: false
});

const PURIFY = {
	USE_PROFILES: { html: true },
	ADD_ATTR: ['target', 'rel', 'class', 'id'],
	ALLOW_DATA_ATTR: false,
	ALLOW_UNKNOWN_PROTOCOLS: false
} as const;

function looksLikeHtmlDocument(text: string): boolean {
	const trimmed = text.trim();
	if (!trimmed.startsWith('<')) return false;
	if (trimmed.includes('```')) return false;
	// Avoid treating `<` comparisons as HTML.
	return /<\/?[a-z][\s\S]*>/i.test(trimmed);
}

/** Split assistant content into markdown / raw HTML / chart segments. */
export function splitRichContent(content: string): RichSegment[] {
	const text = content ?? '';
	if (!text.trim()) return [{ kind: 'markdown', content: '' }];

	const hasFence = /```(chart|html|htm)\b/i.test(text);
	if (looksLikeHtmlDocument(text) && !hasFence) {
		return [{ kind: 'html', content: text.trim() }];
	}
	FENCE.lastIndex = 0;

	const segments: RichSegment[] = [];
	let last = 0;
	let match: RegExpExecArray | null;
	while ((match = FENCE.exec(text)) !== null) {
		const before = text.slice(last, match.index);
		if (before.trim()) segments.push({ kind: 'markdown', content: before });

		const fenceType = (match[1] ?? '').toLowerCase();
		const body = (match[2] ?? '').trim();
		if (fenceType === 'chart') {
			const spec = parseChartSpec(body);
			if (spec) segments.push({ kind: 'chart', spec, raw: body });
			else segments.push({ kind: 'markdown', content: match[0] });
		} else {
			segments.push({ kind: 'html', content: body });
		}
		last = match.index + match[0].length;
	}

	const rest = text.slice(last);
	if (rest.trim() || segments.length === 0) {
		segments.push({ kind: 'markdown', content: rest });
	}
	return segments;
}

/** Backward-compatible chart splitter used by existing chart tests. */
export function splitMarkdownWithCharts(content: string): ContentSegment[] {
	return splitRichContent(content).map((segment) => {
		if (segment.kind === 'html') {
			return { kind: 'markdown', content: `\`\`\`html\n${segment.content}\n\`\`\`` };
		}
		return segment;
	});
}

export function renderMarkdownToSafeHtml(markdown: string): string {
	const raw = markdown ?? '';
	if (!raw.trim()) return '';
	const html = marked.parse(raw, { async: false }) as string;
	// marked often appends a trailing newline after the last block; strip it so
	// chat bubbles don't grow an empty line when white-space is pre-wrap/pre-line.
	return sanitizeHtml(typeof html === 'string' ? html.trim() : html);
}

export function sanitizeHtml(html: string): string {
	const clean = DOMPurify.sanitize(html ?? '', PURIFY);
	return typeof clean === 'string' ? clean : String(clean);
}

export function renderRichSegmentHtml(segment: Extract<RichSegment, { kind: 'markdown' | 'html' }>): string {
	if (segment.kind === 'html') return sanitizeHtml(segment.content);
	return renderMarkdownToSafeHtml(segment.content);
}
