<script lang="ts">
	import { onMount } from 'svelte';
	import { DEFAULT_MODELS } from '$lib/analysis/llm';
	import { t } from '$lib/i18n/i18n.svelte';
	import { llmCatalog, llmSettingsDialog, replaceLlmCatalog } from '$lib/llm/catalog.svelte';
	import {
		createProviderId,
		removeProvider,
		upsertProvider,
		type LlmCatalog,
		type LlmProviderKind,
		type LlmProviderProfile
	} from '$lib/llm/providers';

	function blankProvider(): LlmProviderProfile {
		return {
			id: createProviderId(),
			name: '',
			kind: 'openai',
			apiKey: '',
			model: DEFAULT_MODELS.openai,
			baseUrl: ''
		};
	}

	function snapshot(): LlmCatalog {
		return {
			providers: llmCatalog.providers.map((item) => ({ ...item })),
			selectedId: llmCatalog.selectedId
		};
	}

	const initial =
		llmCatalog.providers.find((item) => item.id === llmCatalog.selectedId) ??
		llmCatalog.providers[0] ??
		null;

	let editingId = $state<string | null>(initial?.id ?? null);
	let draft = $state<LlmProviderProfile>(initial ? { ...initial } : blankProvider());
	let error = $state<string | null>(null);

	function close() {
		llmSettingsDialog.open = false;
	}

	function startCreate() {
		editingId = null;
		error = null;
		draft = blankProvider();
	}

	function startEdit(profile: LlmProviderProfile) {
		editingId = profile.id;
		error = null;
		draft = { ...profile };
	}

	function setKind(kind: LlmProviderKind) {
		if (draft.model.trim() === DEFAULT_MODELS[draft.kind]) {
			draft.model = DEFAULT_MODELS[kind];
		}
		draft.kind = kind;
	}

	function save() {
		const name = draft.name.trim();
		if (!name) {
			error = t('settings.nameRequired');
			return;
		}
		const profile: LlmProviderProfile = {
			...draft,
			name,
			apiKey: draft.apiKey.trim(),
			model: draft.model.trim() || DEFAULT_MODELS[draft.kind],
			baseUrl: draft.baseUrl.trim()
		};
		replaceLlmCatalog(upsertProvider(snapshot(), profile));
		editingId = profile.id;
		draft = { ...profile };
		error = null;
	}

	function remove() {
		if (!editingId) return;
		const next = removeProvider(snapshot(), editingId);
		replaceLlmCatalog(next);
		const fallback = next.providers.find((item) => item.id === next.selectedId) ?? null;
		if (fallback) startEdit(fallback);
		else startCreate();
	}

	function kindLabel(kind: LlmProviderKind): string {
		return kind === 'anthropic' ? t('analysis.provider.anthropic') : t('analysis.provider.openai');
	}

	onMount(() => {
		const onKey = (event: KeyboardEvent) => {
			if (event.key !== 'Escape') return;
			event.preventDefault();
			close();
		};
		window.addEventListener('keydown', onKey, true);
		return () => window.removeEventListener('keydown', onKey, true);
	});
</script>

<div class="modal-backdrop">
	<div
		class="modal settings-dialog"
		role="dialog"
		aria-modal="true"
		aria-labelledby="settings-title"
	>
		<form
			onsubmit={(event) => {
				event.preventDefault();
				save();
			}}
		>
			<header>
				<span id="settings-title">{t('settings.title')}</span>
				<button class="btn" type="button" onclick={close}>{t('settings.close')}</button>
			</header>
			<div class="body">
				<p class="hint">{t('settings.hint')}</p>
				<div class="settings-layout">
					<div class="settings-list">
						<button class="btn" type="button" onclick={startCreate}>{t('settings.add')}</button>
						<div class="settings-list-items">
							{#each llmCatalog.providers as provider (provider.id)}
								<button
									class="settings-provider"
									class:active={editingId === provider.id}
									type="button"
									onclick={() => startEdit(provider)}
								>
									<strong>{provider.name}</strong>
									<small>{kindLabel(provider.kind)} · {provider.model}</small>
								</button>
							{:else}
								<p class="hint">{t('settings.empty')}</p>
							{/each}
						</div>
					</div>
					<div class="settings-form">
						<label class="field">
							<span>{t('settings.name')}</span>
							<input
								bind:value={draft.name}
								autocomplete="off"
								oninput={(event) => {
									if ((event.currentTarget as HTMLInputElement).value.trim()) error = null;
								}}
							/>
						</label>
						<label class="field">
							<span>{t('settings.kind')}</span>
							<select
								value={draft.kind}
								onchange={(event) =>
									setKind((event.currentTarget as HTMLSelectElement).value as LlmProviderKind)}
							>
								<option value="openai">{t('analysis.provider.openai')}</option>
								<option value="anthropic">{t('analysis.provider.anthropic')}</option>
							</select>
						</label>
						<label class="field">
							<span>{t('settings.apiKey')}</span>
							<input type="password" bind:value={draft.apiKey} autocomplete="off" />
						</label>
						<label class="field">
							<span>{t('settings.model')}</span>
							<input bind:value={draft.model} autocomplete="off" spellcheck="false" />
						</label>
						<label class="field">
							<span>{t('settings.baseUrl')}</span>
							<input
								bind:value={draft.baseUrl}
								autocomplete="off"
								spellcheck="false"
								placeholder={draft.kind === 'anthropic'
									? 'https://api.anthropic.com'
									: 'https://api.openai.com/v1'}
							/>
						</label>
						<p class="hint">{t('settings.baseUrlHint')}</p>
						{#if error}<p class="settings-error">{error}</p>{/if}
					</div>
				</div>
			</div>
			<footer>
				<span class="hint">{t('settings.keyLocal')}</span>
				{#if editingId}
					<button class="btn danger" type="button" onclick={remove}>{t('settings.delete')}</button>
				{/if}
				<button class="btn" type="button" onclick={close}>{t('settings.close')}</button>
				<button class="btn primary" type="submit">{t('settings.save')}</button>
			</footer>
		</form>
	</div>
</div>
