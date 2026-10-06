import type { MigrateProgressEvent } from '$lib/api/types';

export type MigrationHistoryStatus = 'success' | 'failed';

export interface MigrationHistorySummary {
	id: string;
	createdAt: string;
	title: string;
	status: MigrationHistoryStatus | string;
	sourceConnection: string;
	sourceName: string;
	targetConnection: string;
	targetName: string;
	engine: string;
	includeData: boolean;
	statementCount: number;
	error?: string | null;
}

export interface MigrationHistoryRecord extends MigrationHistorySummary {
	logs: MigrateProgressEvent[];
}

export interface NewMigrationHistory {
	title: string;
	status: MigrationHistoryStatus;
	sourceConnection: string;
	sourceName: string;
	targetConnection: string;
	targetName: string;
	engine: string;
	includeData: boolean;
	statementCount: number;
	error?: string | null;
	logs: MigrateProgressEvent[];
}

export function migrationHistoryTitle(
	sourceConnection: string,
	sourceName: string,
	targetConnection: string,
	targetName: string
): string {
	return `${sourceConnection} / ${sourceName} → ${targetConnection} / ${targetName}`;
}

export function asMigrationLogs(value: unknown): MigrateProgressEvent[] {
	if (!Array.isArray(value)) return [];
	return value.filter((item): item is MigrateProgressEvent => {
		if (!item || typeof item !== 'object') return false;
		const record = item as Partial<MigrateProgressEvent>;
		return typeof record.phase === 'string' && typeof record.message === 'string';
	});
}
