import type { MessageKey } from '$lib/i18n/en';

export type DemoSource = {
	id: string;
	engine: 'postgres' | 'mysql' | 'mssql' | 'oracle';
	name: string;
	host: string;
	port: number;
	database: string;
	permission: 'rw' | 'ro';
	semanticMappings: number;
	connected: boolean;
};

export type DemoSuggestion = {
	id: string;
	labelKey: MessageKey;
	session: 'analysis-revenue' | 'write-orders';
};

export type DemoInsight = {
	id: string;
	severity: 'high' | 'medium' | 'low';
	titleKey: MessageKey;
	bodyKey: MessageKey;
	sources: string[];
	metrics: string[];
};

export const DEMO_FALLBACK_SOURCES: DemoSource[] = [
	{
		id: 'demo-acme',
		engine: 'postgres',
		name: 'acme-billing',
		host: '10.0.12.8',
		port: 5432,
		database: 'billing',
		permission: 'rw',
		semanticMappings: 12,
		connected: true
	},
	{
		id: 'demo-tidb',
		engine: 'mysql',
		name: 'TiDB Cloud',
		host: 'gateway01.us-west-2.prod.aws.tidbcloud.com',
		port: 4000,
		database: 'commerce',
		permission: 'ro',
		semanticMappings: 8,
		connected: true
	},
	{
		id: 'demo-uniope-psql',
		engine: 'postgres',
		name: 'Uniope-Prod-PSQL',
		host: 'pg.uniope.internal',
		port: 5432,
		database: 'ops',
		permission: 'ro',
		semanticMappings: 5,
		connected: true
	},
	{
		id: 'demo-legacy',
		engine: 'mysql',
		name: 'legacy-mysql',
		host: '192.168.1.44',
		port: 3306,
		database: 'legacy',
		permission: 'ro',
		semanticMappings: 3,
		connected: true
	},
	{
		id: 'demo-sales',
		engine: 'postgres',
		name: 'Uniope-Sales-Prod',
		host: 'sales.pg.internal',
		port: 5432,
		database: 'sales',
		permission: 'rw',
		semanticMappings: 9,
		connected: true
	},
	{
		id: 'demo-analytics',
		engine: 'mysql',
		name: 'analytics-replica',
		host: 'analytics.mysql.internal',
		port: 3306,
		database: 'warehouse',
		permission: 'ro',
		semanticMappings: 6,
		connected: true
	},
	{
		id: 'demo-crm',
		engine: 'postgres',
		name: 'crm-primary',
		host: 'crm.pg.internal',
		port: 5432,
		database: 'crm',
		permission: 'rw',
		semanticMappings: 4,
		connected: true
	},
	{
		id: 'demo-inventory',
		engine: 'mysql',
		name: 'inventory-mysql',
		host: 'inv.mysql.internal',
		port: 3306,
		database: 'stock',
		permission: 'ro',
		semanticMappings: 2,
		connected: true
	}
];

export const DEMO_SUGGESTIONS: DemoSuggestion[] = [
	{ id: 's1', labelKey: 'agentMode.suggest.revenue', session: 'analysis-revenue' },
	{ id: 's2', labelKey: 'agentMode.suggest.refund', session: 'write-orders' },
	{ id: 's3', labelKey: 'agentMode.suggest.weekly', session: 'analysis-revenue' },
	{ id: 's4', labelKey: 'agentMode.suggest.crossJoin', session: 'analysis-revenue' }
];

export const ANALYSIS_CHANNELS: { nameKey: MessageKey; value: number; delta: string }[] = [
	{ nameKey: 'agentMode.channel.ecommerce', value: 98, delta: '+98%' },
	{ nameKey: 'agentMode.channel.agent', value: 62, delta: '+29.5%' },
	{ nameKey: 'agentMode.channel.direct', value: 41, delta: '-13.8%' },
	{ nameKey: 'agentMode.channel.overseas', value: 28, delta: '+6.2%' }
];

export const ANALYSIS_STEPS: {
	id: string;
	titleKey: MessageKey;
	detailKey: MessageKey;
}[] = [
	{ id: '1', titleKey: 'agentMode.trace.intent', detailKey: 'agentMode.trace.intentDetail' },
	{ id: '2', titleKey: 'agentMode.trace.select', detailKey: 'agentMode.trace.selectDetail' },
	{ id: '3', titleKey: 'agentMode.trace.query', detailKey: 'agentMode.trace.queryDetail' },
	{ id: '4', titleKey: 'agentMode.trace.summary', detailKey: 'agentMode.trace.summaryDetail' }
];

export const ANALYSIS_MAPPINGS: { termKey: MessageKey; expr: string }[] = [
	{
		termKey: 'agentMode.term.revenue',
		expr: 'billing.orders.gross_amount - refunds.amount'
	},
	{
		termKey: 'agentMode.term.channel',
		expr: 'sales.dim_channel.name'
	}
];

export const ANALYSIS_TABLES = [
	{ name: 'billing.orders', rows: '2.1M' },
	{ name: 'billing.refunds', rows: '184K' },
	{ name: 'sales.dim_channel', rows: '42' },
	{ name: 'commerce.order_facts', rows: '3.4M' }
];

export const WRITE_PREVIEW_ROWS = [
	{ orderId: 'ORD-88421', refundRate: '11.6%', channel: '电商平台', change: 'Pending → Need Review' },
	{ orderId: 'ORD-88307', refundRate: '9.4%', channel: '代理分销', change: 'Pending → Need Review' },
	{ orderId: 'ORD-88155', refundRate: '8.7%', channel: '直销', change: 'Pending → Need Review' },
	{ orderId: 'ORD-88002', refundRate: '12.1%', channel: '电商平台', change: 'Pending → Need Review' }
];

export const WRITE_IMPACT = [
	{ channel: '电商平台', rows: 642, pct: 50 },
	{ channel: '代理分销', rows: 318, pct: 25 },
	{ channel: '直销', rows: 214, pct: 17 },
	{ channel: '海外', rows: 110, pct: 8 }
];

export const DEMO_INSIGHTS: DemoInsight[] = [
	{
		id: 'ins-1',
		severity: 'high',
		titleKey: 'agentMode.insight.1.title',
		bodyKey: 'agentMode.insight.1.body',
		sources: ['acme-billing', 'TiDB Cloud'],
		metrics: ['Refund rate 11.6%', 'Δ +7.5pp']
	},
	{
		id: 'ins-2',
		severity: 'high',
		titleKey: 'agentMode.insight.2.title',
		bodyKey: 'agentMode.insight.2.body',
		sources: ['Uniope-Sales-Prod'],
		metrics: ['GMV -18%', 'Week over week']
	},
	{
		id: 'ins-3',
		severity: 'medium',
		titleKey: 'agentMode.insight.3.title',
		bodyKey: 'agentMode.insight.3.body',
		sources: ['acme-billing', 'crm-primary'],
		metrics: ['Latency 420ms', 'p95']
	},
	{
		id: 'ins-4',
		severity: 'medium',
		titleKey: 'agentMode.insight.4.title',
		bodyKey: 'agentMode.insight.4.body',
		sources: ['legacy-mysql'],
		metrics: ['Null rate 6.2%']
	},
	{
		id: 'ins-5',
		severity: 'low',
		titleKey: 'agentMode.insight.5.title',
		bodyKey: 'agentMode.insight.5.body',
		sources: ['analytics-replica'],
		metrics: ['Coverage 97%']
	}
];

export const DEMO_HISTORY: {
	id: string;
	titleKey: MessageKey;
	whenKey: MessageKey;
	kind: 'analysis' | 'write';
}[] = [
	{
		id: 'analysis-revenue',
		titleKey: 'agentMode.history.revenue',
		whenKey: 'agentMode.history.justNow',
		kind: 'analysis'
	},
	{
		id: 'write-orders',
		titleKey: 'agentMode.history.write',
		whenKey: 'agentMode.history.today',
		kind: 'write'
	},
	{
		id: 'analysis-revenue',
		titleKey: 'agentMode.history.weekly',
		whenKey: 'agentMode.history.yesterday',
		kind: 'analysis'
	}
];

export const SEMANTIC_LEARNED: { termKey: MessageKey; expr: string }[] = [
	{ termKey: 'agentMode.term.revenue', expr: 'billing.orders.gross_amount - refunds.amount' },
	{ termKey: 'agentMode.term.refundRate', expr: 'refunds.amount / orders.gross_amount' },
	{ termKey: 'agentMode.term.activeCustomer', expr: 'crm.customers.status = active' }
];
