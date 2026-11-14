/** Parsing and analysis of PostgreSQL `EXPLAIN (FORMAT JSON)` output. */

/** Estimated vs actual rows differing by this factor is flagged. */
export const MISESTIMATE_FACTOR = 10;
/** Ignore misestimates on nodes this small; they rarely change the plan. */
const MISESTIMATE_MIN_ROWS = 100;

const DETAIL_KEYS = [
	'Index Cond',
	'Recheck Cond',
	'Hash Cond',
	'Merge Cond',
	'Join Filter',
	'Filter',
	'Rows Removed by Index Recheck',
	'Rows Removed by Join Filter',
	'Rows Removed by Filter',
	'Heap Fetches',
	'Sort Key',
	'Sort Method',
	'Group Key',
	'Presorted Key',
	'Cache Key',
	'Workers Planned',
	'Workers Launched'
] as const;

type RawNode = Record<string, unknown> & { Plans?: RawNode[] };

export interface PlanBuffers {
	sharedHit: number;
	sharedRead: number;
	tempRead: number;
	tempWritten: number;
}

export interface PlanNode {
	id: number;
	depth: number;
	nodeType: string;
	title: string;
	details: string[];
	output: string[];
	startupCost: number;
	totalCost: number;
	exclusiveCost: number;
	planRows: number;
	planWidth: number;
	actualRows: number | null;
	actualLoops: number | null;
	/** Milliseconds across all loops, children included. */
	inclusiveTime: number | null;
	/** Milliseconds spent in this node alone. */
	exclusiveTime: number | null;
	buffers: PlanBuffers | null;
	/** `max(est, actual) / min(est, actual)` when analyzed. */
	estimateFactor: number | null;
	underestimated: boolean;
	misestimated: boolean;
	children: PlanNode[];
}

export interface PlanTrigger {
	name: string;
	relation: string | null;
	time: number;
	calls: number;
}

export interface ParsedPlan {
	root: PlanNode;
	/** Pre-order list of every node. */
	nodes: PlanNode[];
	analyzed: boolean;
	planningTime: number | null;
	executionTime: number | null;
	triggers: PlanTrigger[];
	/** Node with the most exclusive time (analyzed) or cost (estimate only). */
	hottestId: number | null;
}

function num(value: unknown): number | null {
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function str(value: unknown): string | null {
	return typeof value === 'string' && value ? value : null;
}

function detailText(value: unknown): string | null {
	if (Array.isArray(value)) return value.length ? value.join(', ') : null;
	if (typeof value === 'number') return String(value);
	return str(value);
}

function relationLabel(raw: RawNode): string | null {
	const relation = str(raw['Relation Name']);
	if (!relation) return null;
	const schema = str(raw['Schema']);
	const alias = str(raw['Alias']);
	const qualified = schema ? `${schema}.${relation}` : relation;
	return alias && alias !== relation ? `${qualified} ${alias}` : qualified;
}

const AGGREGATE_NAMES: Record<string, string> = {
	Hashed: 'HashAggregate',
	Sorted: 'GroupAggregate',
	Mixed: 'MixedAggregate'
};

/** Node label in the style of psql's text EXPLAIN. */
export function nodeTitle(raw: RawNode): string {
	const nodeType = str(raw['Node Type']) ?? 'Unknown';
	let title = nodeType;
	const joinType = str(raw['Join Type']);
	if (joinType && joinType !== 'Inner') {
		title = nodeType.endsWith(' Join')
			? nodeType.replace(/ Join$/, ` ${joinType} Join`)
			: `${nodeType} ${joinType} Join`;
	}
	if (nodeType === 'Aggregate') {
		title = AGGREGATE_NAMES[str(raw['Strategy']) ?? ''] ?? 'Aggregate';
	}
	if (nodeType === 'ModifyTable') {
		title = str(raw['Operation']) ?? nodeType;
	}
	if (raw['Scan Direction'] === 'Backward') title += ' Backward';
	const index = str(raw['Index Name']);
	if (index) title += ` using ${index}`;
	const target =
		relationLabel(raw) ??
		str(raw['CTE Name']) ??
		str(raw['Function Name']) ??
		(nodeType === 'Subquery Scan' ? str(raw['Alias']) : null);
	if (target) title += ` on ${target}`;
	const subplan = str(raw['Subplan Name']);
	return subplan ? `${subplan} → ${title}` : title;
}

function buffersOf(raw: RawNode): PlanBuffers | null {
	const sharedHit = num(raw['Shared Hit Blocks']);
	const sharedRead = num(raw['Shared Read Blocks']);
	if (sharedHit == null && sharedRead == null) return null;
	return {
		sharedHit: sharedHit ?? 0,
		sharedRead: sharedRead ?? 0,
		tempRead: num(raw['Temp Read Blocks']) ?? 0,
		tempWritten: num(raw['Temp Written Blocks']) ?? 0
	};
}

export function estimateFactor(planRows: number, actualRows: number): number {
	const high = Math.max(planRows, actualRows);
	const low = Math.max(Math.min(planRows, actualRows), 1);
	return high / low;
}

function buildNode(raw: RawNode, depth: number, nextId: { value: number }): PlanNode {
	const id = nextId.value++;
	const children = (raw.Plans ?? []).map((child) => buildNode(child, depth + 1, nextId));
	const totalCost = num(raw['Total Cost']) ?? 0;
	const planRows = num(raw['Plan Rows']) ?? 0;
	const actualRows = num(raw['Actual Rows']);
	const actualLoops = num(raw['Actual Loops']);
	const perLoopTime = num(raw['Actual Total Time']);
	const inclusiveTime =
		perLoopTime != null && actualLoops != null ? perLoopTime * actualLoops : null;
	const childTime = children.reduce((sum, child) => sum + (child.inclusiveTime ?? 0), 0);
	const childCost = children.reduce((sum, child) => sum + child.totalCost, 0);

	let factor: number | null = null;
	let misestimated = false;
	if (actualRows != null && actualLoops != null && actualLoops > 0) {
		factor = estimateFactor(planRows, actualRows);
		misestimated =
			factor >= MISESTIMATE_FACTOR &&
			Math.max(planRows, actualRows) * actualLoops >= MISESTIMATE_MIN_ROWS;
	}

	const details: string[] = [];
	for (const key of DETAIL_KEYS) {
		const text = detailText(raw[key]);
		if (text != null && !(key.startsWith('Rows Removed') && text === '0')) {
			details.push(`${key}: ${text}`);
		}
	}

	return {
		id,
		depth,
		nodeType: str(raw['Node Type']) ?? 'Unknown',
		title: nodeTitle(raw),
		details,
		output: Array.isArray(raw['Output']) ? (raw['Output'] as unknown[]).map(String) : [],
		startupCost: num(raw['Startup Cost']) ?? 0,
		totalCost,
		exclusiveCost: Math.max(totalCost - childCost, 0),
		planRows,
		planWidth: num(raw['Plan Width']) ?? 0,
		actualRows,
		actualLoops,
		inclusiveTime,
		exclusiveTime: inclusiveTime == null ? null : Math.max(inclusiveTime - childTime, 0),
		buffers: buffersOf(raw),
		estimateFactor: factor,
		underestimated: actualRows != null && actualRows > planRows,
		misestimated,
		children
	};
}

function flatten(node: PlanNode, out: PlanNode[] = []): PlanNode[] {
	out.push(node);
	for (const child of node.children) flatten(child, out);
	return out;
}

/** Parse the JSON document returned by `EXPLAIN (FORMAT JSON …)`. */
export function parsePgPlan(json: string): ParsedPlan {
	const parsed: unknown = JSON.parse(json);
	const document = (Array.isArray(parsed) ? parsed[0] : parsed) as Record<string, unknown> | null;
	const rawRoot = document?.['Plan'] as RawNode | undefined;
	if (!rawRoot || typeof rawRoot !== 'object') {
		throw new Error('EXPLAIN output has no plan');
	}
	const root = buildNode(rawRoot, 0, { value: 0 });
	const nodes = flatten(root);
	const analyzed = root.actualLoops != null;
	const metric = (node: PlanNode) => (analyzed ? (node.exclusiveTime ?? 0) : node.exclusiveCost);
	const hottest = nodes.reduce<PlanNode | null>(
		(best, node) => (best == null || metric(node) > metric(best) ? node : best),
		null
	);
	const triggers = Array.isArray(document?.['Triggers'])
		? (document['Triggers'] as Record<string, unknown>[]).map((trigger) => ({
				name: str(trigger['Trigger Name']) ?? '',
				relation: str(trigger['Relation']),
				time: num(trigger['Time']) ?? 0,
				calls: num(trigger['Calls']) ?? 0
			}))
		: [];
	return {
		root,
		nodes,
		analyzed,
		planningTime: num(document?.['Planning Time']),
		executionTime: num(document?.['Execution Time']),
		triggers,
		hottestId: hottest && metric(hottest) > 0 ? hottest.id : null
	};
}

/** Share of the whole plan spent in `node` (time when analyzed, else cost), 0–1. */
export function nodeShare(plan: ParsedPlan, node: PlanNode): number {
	if (plan.analyzed) {
		const total = plan.root.inclusiveTime ?? 0;
		return total > 0 ? (node.exclusiveTime ?? 0) / total : 0;
	}
	const total = plan.root.totalCost;
	return total > 0 ? node.exclusiveCost / total : 0;
}
