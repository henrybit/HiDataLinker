<script lang="ts">
	import { onDestroy } from 'svelte';
	import { Chart as G2Chart } from '@antv/g2';
	import type { EChartsType } from 'echarts/core';
	import * as echarts from 'echarts/core';
	import { BarChart, LineChart, PieChart } from 'echarts/charts';
	import {
		GridComponent,
		LegendComponent,
		TitleComponent,
		TooltipComponent
	} from 'echarts/components';
	import { CanvasRenderer } from 'echarts/renderers';
	import type { AgentChartSpec, ChartEngine } from '$lib/agent/chart-spec';
	import { t } from '$lib/i18n/i18n.svelte';

	echarts.use([
		BarChart,
		LineChart,
		PieChart,
		GridComponent,
		LegendComponent,
		TitleComponent,
		TooltipComponent,
		CanvasRenderer
	]);

	let {
		spec,
		engine = 'echarts'
	}: {
		spec: AgentChartSpec;
		engine?: ChartEngine;
	} = $props();

	let host = $state<HTMLDivElement | null>(null);
	let echart: EChartsType | null = null;
	let g2: InstanceType<typeof G2Chart> | null = null;
	let resizeObserver: ResizeObserver | null = null;

	function dispose() {
		resizeObserver?.disconnect();
		resizeObserver = null;
		echart?.dispose();
		echart = null;
		g2?.destroy();
		g2 = null;
	}

	function toEchartsOption(input: AgentChartSpec) {
		const title = input.title
			? { text: input.title, left: 'center', textStyle: { fontSize: 13, fontWeight: 600 } }
			: undefined;
		if (input.type === 'pie') {
			return {
				title,
				tooltip: { trigger: 'item' },
				legend: { bottom: 0, type: 'scroll' },
				series: [
					{
						type: 'pie',
						radius: ['36%', '64%'],
						center: ['50%', '48%'],
						data: (input.data ?? []).map((item) => ({
							name: item.name,
							value: item.value
						})),
						label: { formatter: '{b}: {d}%' }
					}
				]
			};
		}
		const seriesType = input.type === 'area' ? 'line' : input.type;
		return {
			title,
			tooltip: { trigger: 'axis' },
			legend: { bottom: 0, type: 'scroll' },
			grid: { left: 48, right: 16, top: input.title ? 40 : 24, bottom: 48 },
			xAxis: {
				type: 'category',
				data: input.categories ?? [],
				axisLabel: { hideOverlap: true }
			},
			yAxis: {
				type: 'value',
				name: input.unit,
				splitLine: { lineStyle: { type: 'dashed' } }
			},
			series: (input.series ?? []).map((item) => ({
				name: item.name,
				type: seriesType,
				data: item.data,
				smooth: input.type === 'line' || input.type === 'area',
				areaStyle: input.type === 'area' ? {} : undefined,
				emphasis: { focus: 'series' }
			}))
		};
	}

	function cartesianRows(input: AgentChartSpec) {
		const rows: { category: string; value: number; series: string }[] = [];
		const categories = input.categories ?? [];
		for (const series of input.series ?? []) {
			series.data.forEach((value, index) => {
				rows.push({
					category: categories[index] ?? String(index + 1),
					value,
					series: series.name
				});
			});
		}
		return rows;
	}

	function renderEcharts(el: HTMLDivElement, input: AgentChartSpec) {
		echart = echarts.init(el, undefined, { renderer: 'canvas' });
		echart.setOption(toEchartsOption(input), true);
	}

	function renderAntv(el: HTMLDivElement, input: AgentChartSpec) {
		g2 = new G2Chart({
			container: el,
			autoFit: true,
			height: 280
		});

		if (input.type === 'pie') {
			g2
				.coordinate({ type: 'theta', outerRadius: 0.8, innerRadius: 0.5 })
				.interval()
				.data(input.data ?? [])
				.transform({ type: 'stackY' })
				.encode('y', 'value')
				.encode('color', 'name')
				.legend('color', { position: 'bottom' })
				.tooltip((d: { name: string; value: number }) => ({
					name: d.name,
					value: d.value
				}));
			if (input.title) g2.title(input.title);
		} else {
			const mark =
				input.type === 'bar' ? g2.interval() : input.type === 'area' ? g2.area() : g2.line();
			mark
				.data(cartesianRows(input))
				.encode('x', 'category')
				.encode('y', 'value')
				.encode('color', 'series')
				.encode('series', 'series')
				.legend('color', { position: 'bottom' })
				.axis('y', { title: input.unit || false })
				.axis('x', { title: false });
			if (input.title) g2.title(input.title);
		}

		void g2.render();
	}

	function render() {
		if (!host) return;
		dispose();
		host.replaceChildren();
		const canvasHost = document.createElement('div');
		canvasHost.className = 'agent-chart-canvas';
		host.appendChild(canvasHost);

		try {
			if (engine === 'antd') {
				renderAntv(canvasHost, spec);
			} else {
				renderEcharts(canvasHost, spec);
			}
			resizeObserver = new ResizeObserver(() => {
				echart?.resize();
				if (g2 && host) {
					const width = Math.max(host.clientWidth, 160);
					void g2.changeSize(width, 280).catch(() => undefined);
				}
			});
			resizeObserver.observe(host);
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			host.innerHTML = `<p class="agent-chart-error">${message}</p>`;
		}
	}

	$effect(() => {
		spec;
		engine;
		host;
		queueMicrotask(render);
		return () => dispose();
	});

	onDestroy(dispose);
</script>

<figure class="agent-chart" data-engine={engine}>
	<div class="agent-chart-host" bind:this={host}></div>
	<figcaption class="agent-chart-engine">
		{engine === 'antd' ? t('agentMode.chart.engineAntd') : t('agentMode.chart.engineEcharts')}
	</figcaption>
</figure>
