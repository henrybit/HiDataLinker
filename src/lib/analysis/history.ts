import type { RelationshipGraph, SchemaScope } from './types';

export interface AnalysisHistorySummary {
	id: string;
	createdAt: string;
	title: string;
	objectCount: number;
	edgeCount: number;
	scopes: string[];
	providerName: string;
	model: string;
}

export interface AnalysisHistoryRecord extends AnalysisHistorySummary {
	graph: RelationshipGraph;
}

export interface NewAnalysisHistory {
	title: string;
	scopes: string[];
	providerName: string;
	model: string;
	graph: RelationshipGraph;
}

export function analysisHistoryTitle(scopes: SchemaScope[]): string {
	const labels = scopes.map((scope) => `${scope.connectionName} / ${scope.schema}`);
	if (labels.length === 0) return '';
	if (labels.length <= 3) return labels.join(', ');
	return `${labels.slice(0, 3).join(', ')} +${labels.length - 3}`;
}

export function asRelationshipGraph(value: unknown): RelationshipGraph | null {
	if (!value || typeof value !== 'object') return null;
	const record = value as Partial<RelationshipGraph>;
	if (!Array.isArray(record.nodes) || !Array.isArray(record.edges)) return null;
	return {
		nodes: record.nodes,
		edges: record.edges,
		warnings: Array.isArray(record.warnings) ? record.warnings : []
	};
}
