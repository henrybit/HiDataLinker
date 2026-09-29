import { api, isTauriRuntime } from '$lib/api/tauri';
import { en, type MessageKey } from './en';
import { zh } from './zh';
import { isSchemaScoped } from '$lib/engine';
import type { FolderKind } from '$lib/api/types';

export type { MessageKey };
export type Locale = 'en' | 'zh';

const catalogs: Record<Locale, Record<MessageKey, string>> = { en: { ...en }, zh };

const STORAGE_KEY = 'db-gui-locale';

function readStoredLocale(): Locale {
	if (typeof localStorage === 'undefined') return 'en';
	const saved = localStorage.getItem(STORAGE_KEY);
	return saved === 'zh' || saved === 'en' ? saved : 'en';
}

const initialLocale = readStoredLocale();
let locale = $state<Locale>(initialLocale);

export function getLocale(): Locale {
	return locale;
}

export function localeTag(): string {
	return locale === 'zh' ? 'zh-CN' : 'en-US';
}

export function setLocale(next: Locale) {
	if (locale === next) return;
	locale = next;
	if (isTauriRuntime()) {
		void api.writeLocale(next).catch((error) => {
			console.error('[storage] locale', error);
		});
	} else {
		try {
			localStorage.setItem(STORAGE_KEY, next);
		} catch {
			/* ignore quota / private mode */
		}
	}
	applyDocumentLang(next);
}

export async function hydrateLocale() {
	const stored = (await api.readLocale()).trim();
	if (stored === 'en' || stored === 'zh') {
		setLocale(stored);
		return;
	}
	await api.writeLocale(getLocale());
}

function applyDocumentLang(next: Locale) {
	if (typeof document !== 'undefined') {
		document.documentElement.lang = next === 'zh' ? 'zh-CN' : 'en';
	}
}

export type MessageParams = Record<string, string | number>;

export function t(key: MessageKey, params?: MessageParams): string {
	const dict = catalogs[locale] ?? catalogs.en;
	let text = dict[key] ?? catalogs.en[key] ?? key;
	if (params) {
		for (const [name, value] of Object.entries(params)) {
			text = text.replaceAll(`{${name}}`, String(value));
		}
	}
	return text;
}

export function schemaNounLabel(engine: string | undefined | null): string {
	return t(isSchemaScoped(engine) ? 'noun.schema' : 'noun.database');
}

export function schemaNounLower(engine: string | undefined | null): string {
	return t(isSchemaScoped(engine) ? 'noun.schema.lower' : 'noun.database.lower');
}

export function folderMessageKey(folder: FolderKind): MessageKey {
	switch (folder) {
		case 'tables':
			return 'folder.tables';
		case 'views':
			return 'folder.views';
		case 'indexes':
			return 'folder.indexes';
		case 'triggers':
			return 'folder.triggers';
		case 'functions':
			return 'folder.functions';
	}
}

applyDocumentLang(initialLocale);
