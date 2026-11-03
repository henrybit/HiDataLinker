export type ChartEngine = 'echarts' | 'antd';

export type AgentChartType = 'bar' | 'line' | 'area' | 'pie';

export type AgentChartSeries = {
	name: string;
	data: number[];
};

export type AgentChartSpec = {
	type: AgentChartType;
	title?: string;
	unit?: string;
	categories?: string[];
	series?: AgentChartSeries[];
	/** Pie slices */
	data?: { name: string; value: number }[];
};

export type ContentSegment =
	| { kind: 'markdown'; content: string }
	| { kind: 'chart'; spec: AgentChartSpec; raw: string };

const CHART_FENCE = /```chart\s*\n([\s\S]*?)```/gi;

function asFiniteNumber(value: unknown): number | null {
	if (typeof value === 'number' && Number.isFinite(value)) return value;
	if (typeof value === 'string' && value.trim() !== '') {
		const n = Number(value);
		return Number.isFinite(n) ? n : null;
	}
	return null;
}

export function parseChartSpec(raw: string): AgentChartSpec | null {
	try {
		const parsed = JSON.parse(raw) as unknown;
		if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null;
		const obj = parsed as Record<string, unknown>;
		const type = obj.type;
		if (type !== 'bar' && type !== 'line' && type !== 'area' && type !== 'pie') return null;

		const title = typeof obj.title === 'string' ? obj.title : undefined;
		const unit = typeof obj.unit === 'string' ? obj.unit : undefined;

		if (type === 'pie') {
			const rows = Array.isArray(obj.data) ? obj.data : [];
			const data = rows
				.map((row) => {
					if (!row || typeof row !== 'object') return null;
					const item = row as Record<string, unknown>;
					const name = String(item.name ?? item.label ?? '').trim();
					const value = asFiniteNumber(item.value);
					if (!name || value == null) return null;
					return { name, value };
				})
				.filter((item): item is { name: string; value: number } => !!item);
			if (data.length === 0) return null;
			return { type, title, unit, data };
		}

		const categories = Array.isArray(obj.categories)
			? obj.categories.map((item) => String(item))
			: [];
		const seriesRaw = Array.isArray(obj.series) ? obj.series : [];
		const series = seriesRaw
			.map((row, index) => {
				if (!row || typeof row !== 'object') return null;
				const item = row as Record<string, unknown>;
				const name = String(item.name ?? `series ${index + 1}`).trim() || `series ${index + 1}`;
				const values = Array.isArray(item.data)
					? item.data.map((v) => asFiniteNumber(v) ?? 0)
					: [];
				if (values.length === 0) return null;
				return { name, data: values };
			})
			.filter((item): item is AgentChartSeries => !!item);
		if (series.length === 0) return null;
		const width = Math.max(...series.map((item) => item.data.length), categories.length);
		const cats =
			categories.length >= width
				? categories.slice(0, width)
				: [
						...categories,
						...Array.from({ length: width - categories.length }, (_, i) =>
							String(categories.length + i + 1)
						)
					];
		return {
			type,
			title,
			unit,
			categories: cats,
			series: series.map((item) => ({
				name: item.name,
				data: item.data.slice(0, cats.length)
			}))
		};
	} catch {
		return null;
	}
}

/** @deprecated Prefer `splitRichContent` from `$lib/agent/rich-content`. */
export function splitMarkdownWithCharts(content: string): ContentSegment[] {
	const text = content ?? '';
	if (!text.trim()) return [{ kind: 'markdown', content: '' }];

	const segments: ContentSegment[] = [];
	let last = 0;
	CHART_FENCE.lastIndex = 0;
	let match: RegExpExecArray | null;
	while ((match = CHART_FENCE.exec(text)) !== null) {
		const before = text.slice(last, match.index);
		if (before.trim()) segments.push({ kind: 'markdown', content: before });
		const raw = match[1]?.trim() ?? '';
		const spec = parseChartSpec(raw);
		if (spec) {
			segments.push({ kind: 'chart', spec, raw });
		} else {
			segments.push({ kind: 'markdown', content: match[0] });
		}
		last = match.index + match[0].length;
	}
	const rest = text.slice(last);
	if (rest.trim() || segments.length === 0) {
		segments.push({ kind: 'markdown', content: rest });
	}
	return segments;
}

export function isChartEngine(value: unknown): value is ChartEngine {
	return value === 'echarts' || value === 'antd';
}
