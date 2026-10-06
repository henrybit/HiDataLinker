import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
import type { MigrateProgressEvent } from '$lib/api/types';
import {
	migrationHistoryTitle,
	type MigrationHistoryStatus,
	type MigrationHistorySummary,
	type NewMigrationHistory
} from './history';

export interface RememberMigrationInput {
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

export async function rememberMigration(
	input: RememberMigrationInput
): Promise<{ saved?: MigrationHistorySummary; warning?: string }> {
	if (!isTauriRuntime()) return {};
	const request: NewMigrationHistory = {
		title: migrationHistoryTitle(
			input.sourceConnection,
			input.sourceName,
			input.targetConnection,
			input.targetName
		),
		status: input.status,
		sourceConnection: input.sourceConnection,
		sourceName: input.sourceName,
		targetConnection: input.targetConnection,
		targetName: input.targetName,
		engine: input.engine,
		includeData: input.includeData,
		statementCount: input.statementCount,
		error: input.error ?? null,
		logs: input.logs
	};
	try {
		const saved = await api.saveMigrationHistory(request);
		return { saved };
	} catch (caught) {
		return { warning: errorMessage(caught) };
	}
}
