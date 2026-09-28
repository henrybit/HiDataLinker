import {
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

export function selectedLlmProvider(): LlmProviderProfile | null {
	return llmCatalog.providers.find((item) => item.id === llmCatalog.selectedId) ?? null;
}
