import { t } from '$lib/i18n/i18n.svelte';

export function migratePhaseLabel(phase: string): string {
	switch (phase) {
		case 'validate':
			return t('dialog.migratePhaseValidate');
		case 'dump':
			return t('dialog.migratePhaseDump');
		case 'execute':
			return t('dialog.migratePhaseExecute');
		case 'rename':
			return t('dialog.migratePhaseRename');
		case 'done':
			return t('dialog.migratePhaseDone');
		default:
			return phase;
	}
}

export function migrateLevelClass(level: string): string {
	if (level === 'error') return 'error';
	if (level === 'success') return 'success';
	if (level === 'warning') return 'warning';
	return 'info';
}

export function migrationStatusLabel(status: string): string {
	if (status === 'success') return t('migration.status.success');
	if (status === 'failed') return t('migration.status.failed');
	return status;
}
