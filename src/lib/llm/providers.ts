import { DEFAULT_MODELS, type LlmProvider, type LlmSettings } from '$lib/analysis/llm';
import { api, isTauriRuntime } from '$lib/api/tauri';

export type LlmProviderKind = LlmProvider;

export interface LlmProviderProfile {
	id: string;
	name: string;
	kind: LlmProviderKind;
	apiKey: string;
	model: string;
	baseUrl: string;
}

export interface LlmCatalog {
	providers: LlmProviderProfile[];
	selectedId: string | null;
}

const STORAGE_KEY = 'db-gui-llm-providers';
const LEGACY_KEY = 'db-gui-relationship-llm';

export function emptyCatalog(): LlmCatalog {
	return { providers: [], selectedId: null };
}

export function createProviderId(): string {
	return crypto.randomUUID();
}

export function toLlmSettings(profile: LlmProviderProfile): LlmSettings {
	return {
		provider: profile.kind,
		apiKey: profile.apiKey,
		model: profile.model.trim() || DEFAULT_MODELS[profile.kind],
		baseUrl: profile.baseUrl
	};
}

export function readCatalog(
	current: string | null,
	legacy: string | null,
	createId: () => string = createProviderId
): LlmCatalog {
	const parsed = current ? parseCatalogJson(current) : null;
	if (parsed) return parsed;
	if (legacy) {
		const migrated = parseLegacy(legacy, createId);
		if (migrated) return migrated;
	}
	return emptyCatalog();
}

export function catalogJson(catalog: LlmCatalog): string {
	return JSON.stringify({
		providers: catalog.providers,
		selectedId: catalog.selectedId
	});
}

/** Prefer the home-directory file. Browser storage is only a one-time migration source. */
export function resolveStoredCatalog(
	fileText: string,
	browserCurrent: string | null,
	legacy: string | null
): { catalog: LlmCatalog; writeFile: boolean } {
	if (fileText.trim()) {
		const parsed = parseCatalogJson(fileText);
		if (parsed) return { catalog: parsed, writeFile: false };
	}
	return { catalog: readCatalog(browserCurrent, legacy), writeFile: true };
}

export function loadLlmCatalog(): LlmCatalog {
	if (typeof localStorage === 'undefined') return emptyCatalog();
	const current = localStorage.getItem(STORAGE_KEY);
	const catalog = readCatalog(current, localStorage.getItem(LEGACY_KEY));
	if (current == null && catalog.providers.length > 0) saveLlmCatalog(catalog);
	return catalog;
}

export function saveLlmCatalog(catalog: LlmCatalog) {
	const json = catalogJson(catalog);
	if (isTauriRuntime()) {
		void api.writeLlmCatalog(json).catch((error) => {
			console.error('[storage] llm catalog', error);
		});
		return;
	}
	if (typeof localStorage === 'undefined') return;
	localStorage.setItem(STORAGE_KEY, json);
}

export async function hydrateLlmCatalog(): Promise<LlmCatalog> {
	const fileText = await api.readLlmCatalog();
	const browserCurrent =
		typeof localStorage === 'undefined' ? null : localStorage.getItem(STORAGE_KEY);
	const legacy = typeof localStorage === 'undefined' ? null : localStorage.getItem(LEGACY_KEY);
	const resolved = resolveStoredCatalog(fileText, browserCurrent, legacy);
	if (resolved.writeFile) await api.writeLlmCatalog(catalogJson(resolved.catalog));
	return resolved.catalog;
}

export function upsertProvider(catalog: LlmCatalog, profile: LlmProviderProfile): LlmCatalog {
	const next = normalizeProfile(profile);
	if (!next) return catalog;
	const exists = catalog.providers.some((item) => item.id === next.id);
	const providers = exists
		? catalog.providers.map((item) => (item.id === next.id ? next : item))
		: [...catalog.providers, next];
	const selectedStillThere =
		catalog.selectedId != null && providers.some((item) => item.id === catalog.selectedId);
	return {
		providers,
		selectedId: selectedStillThere ? catalog.selectedId : next.id
	};
}

export function removeProvider(catalog: LlmCatalog, id: string): LlmCatalog {
	const providers = catalog.providers.filter((item) => item.id !== id);
	const selectedId =
		catalog.selectedId !== id && providers.some((item) => item.id === catalog.selectedId)
			? catalog.selectedId
			: (providers[0]?.id ?? null);
	return { providers, selectedId };
}

function parseCatalogJson(raw: string): LlmCatalog | null {
	try {
		const parsed = JSON.parse(raw) as { providers?: unknown; selectedId?: unknown };
		if (!parsed || typeof parsed !== 'object' || !Array.isArray(parsed.providers)) return null;
		const providers: LlmProviderProfile[] = [];
		const seen = new Set<string>();
		for (const item of parsed.providers) {
			const profile = normalizeProfile(item);
			if (!profile || seen.has(profile.id)) continue;
			seen.add(profile.id);
			providers.push(profile);
		}
		const selectedId =
			typeof parsed.selectedId === 'string' && seen.has(parsed.selectedId)
				? parsed.selectedId
				: (providers[0]?.id ?? null);
		return { providers, selectedId };
	} catch {
		return null;
	}
}

function parseLegacy(raw: string, createId: () => string): LlmCatalog | null {
	try {
		const parsed = JSON.parse(raw) as Partial<LlmSettings>;
		if (!parsed || typeof parsed !== 'object') return null;
		const kind: LlmProviderKind = parsed.provider === 'anthropic' ? 'anthropic' : 'openai';
		const apiKey = typeof parsed.apiKey === 'string' ? parsed.apiKey : '';
		const model = typeof parsed.model === 'string' ? parsed.model.trim() : '';
		const baseUrl = typeof parsed.baseUrl === 'string' ? parsed.baseUrl : '';
		const configured =
			apiKey.trim() !== '' ||
			baseUrl.trim() !== '' ||
			(model !== '' && model !== DEFAULT_MODELS[kind]);
		if (!configured) return null;
		const id = createId();
		return {
			providers: [
				{
					id,
					name: kind === 'anthropic' ? 'Anthropic' : 'OpenAI',
					kind,
					apiKey,
					model: model || DEFAULT_MODELS[kind],
					baseUrl
				}
			],
			selectedId: id
		};
	} catch {
		return null;
	}
}

function normalizeProfile(value: unknown): LlmProviderProfile | null {
	if (!value || typeof value !== 'object') return null;
	const item = value as Record<string, unknown>;
	const kind: LlmProviderKind | null =
		item.kind === 'anthropic' ? 'anthropic' : item.kind === 'openai' ? 'openai' : null;
	if (!kind) return null;
	const id = typeof item.id === 'string' ? item.id.trim() : '';
	if (!id) return null;
	const name = typeof item.name === 'string' ? item.name.trim() : '';
	const model = typeof item.model === 'string' ? item.model.trim() : '';
	return {
		id,
		name: name || (kind === 'anthropic' ? 'Anthropic' : 'OpenAI'),
		kind,
		apiKey: typeof item.apiKey === 'string' ? item.apiKey : '',
		model: model || DEFAULT_MODELS[kind],
		baseUrl: typeof item.baseUrl === 'string' ? item.baseUrl : ''
	};
}
