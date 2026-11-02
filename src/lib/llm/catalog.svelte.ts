import {
	hydrateLlmCatalog,
	loadLlmCatalog,
	saveLlmCatalog,
	type LlmCatalog,
	type LlmProviderProfile
} from './providers';

const loaded = loadLlmCatalog();

export const llmCatalog = $state<LlmCatalog>({
	providers: loaded.providers,
	selectedId: loaded.selectedId
});

export const llmSettingsDialog = $state({ open: false });

export async function hydrateLlmCatalogFromDisk() {
	const next = await hydrateLlmCatalog();
	llmCatalog.providers = next.providers;
	llmCatalog.selectedId = next.selectedId;
}

export function replaceLlmCatalog(next: LlmCatalog) {
	llmCatalog.providers = next.providers;
	llmCatalog.selectedId = next.selectedId;
	saveLlmCatalog(llmCatalog);
}

export function selectLlmProvider(id: string) {
	if (!llmCatalog.providers.some((item) => item.id === id)) return;
	llmCatalog.selectedId = id;
	saveLlmCatalog(llmCatalog);
}

/** Selected provider, or the first catalog entry as the app default. */
export function selectedLlmProvider(): LlmProviderProfile | null {
	const selected = llmCatalog.providers.find((item) => item.id === llmCatalog.selectedId);
	return selected ?? llmCatalog.providers[0] ?? null;
}
