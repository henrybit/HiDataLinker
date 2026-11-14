import { api, isTauriRuntime } from '$lib/api/tauri';
import { downloadTextFile } from '$lib/download';

export function sqlDumpFileName(base: string, suffix = ''): string {
	const safe = base.trim().replace(/[\\/:*?"<>|]+/g, '_') || 'export';
	return `${safe}${suffix}.sql`;
}

/** Ask the user where to save; null when they cancel. Nothing is written here. */
export async function pickSavePath(
	defaultFileName: string,
	title: string,
	filter: { name: string; extensions: string[] }
): Promise<string | null> {
	if (!isTauriRuntime()) return defaultFileName;
	const { save } = await import('@tauri-apps/plugin-dialog');
	const selected = await save({ title, defaultPath: defaultFileName, filters: [filter] });
	return selected ?? null;
}

export async function pickSqlSavePath(
	defaultFileName: string,
	title = 'Export SQL'
): Promise<string | null> {
	return pickSavePath(defaultFileName, title, { name: 'SQL', extensions: ['sql'] });
}

export async function writeSqlFile(
	path: string,
	content: string,
	fallbackName: string,
	mime = 'application/sql'
): Promise<string> {
	if (!isTauriRuntime()) {
		downloadTextFile(fallbackName, content, mime);
		return fallbackName;
	}
	return api.writeTextFile(path, content);
}
