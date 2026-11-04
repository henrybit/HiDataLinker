<script lang="ts">
	import { onMount } from 'svelte';
	import { DEFAULT_MODELS } from '$lib/analysis/llm';
	import {
		agentGateway,
		refreshAgentGateway,
		setAgentGatewayEnabled
	} from '$lib/agent/gateway.svelte';
	import { api, isTauriRuntime } from '$lib/api/tauri';
	import Switch from '$lib/components/layout/Switch.svelte';
	import { t } from '$lib/i18n/i18n.svelte';
	import { llmCatalog, llmSettingsDialog, replaceLlmCatalog } from '$lib/llm/catalog.svelte';
	import {
		DEFAULT_CONTEXT_WINDOW,
		formatTokenCount,
		normalizeContextWindow
	} from '$lib/llm/context-window';
	import {
		modalityI18nKey,
		normalizeSelectableModality,
		resolveProfileModality,
		type SelectableModelModality
	} from '$lib/llm/modality';
	import {
		createProviderId,
		removeProvider,
		upsertProvider,
		type LlmCatalog,
		type LlmProviderKind,
		type LlmProviderProfile
	} from '$lib/llm/providers';

	type SettingsTab = 'basic' | 'llm';

	function blankProvider(): LlmProviderProfile {
		const model = DEFAULT_MODELS.openai;
		return {
			id: createProviderId(),
			name: '',
			kind: 'openai',
			apiKey: '',
			model,
			baseUrl: '',
			contextWindow: DEFAULT_CONTEXT_WINDOW,
			modality: normalizeSelectableModality(undefined, model)
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

	let tab = $state<SettingsTab>('basic');
	let editingId = $state<string | null>(initial?.id ?? null);
	let draft = $state<LlmProviderProfile>(
		initial
			? {
					...initial,
					contextWindow: normalizeContextWindow(initial.contextWindow),
					modality: normalizeSelectableModality(initial.modality, initial.model)
				}
			: blankProvider()
	);
	let error = $state<string | null>(null);
	let fileLoggingEnabled = $state(true);
	let runtimeLogBusy = $state(false);
	let runtimeLogError = $state<string | null>(null);

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
		draft = {
			...profile,
			contextWindow: normalizeContextWindow(profile.contextWindow),
			modality: normalizeSelectableModality(profile.modality, profile.model)
		};
	}

	function setKind(kind: LlmProviderKind) {
		if (draft.model.trim() === DEFAULT_MODELS[draft.kind]) {
			draft.model = DEFAULT_MODELS[kind];
			draft.modality = normalizeSelectableModality(undefined, draft.model);
		}
		draft.kind = kind;
	}

	function setModality(modality: SelectableModelModality) {
		draft.modality = modality;
	}

	function save() {
		const name = draft.name.trim();
		if (!name) {
			error = t('settings.nameRequired');
			return;
		}
		const model = draft.model.trim() || DEFAULT_MODELS[draft.kind];
		const profile: LlmProviderProfile = {
			...draft,
			name,
			apiKey: draft.apiKey.trim(),
			model,
			baseUrl: draft.baseUrl.trim(),
			contextWindow: normalizeContextWindow(draft.contextWindow),
			modality: normalizeSelectableModality(draft.modality, model)
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

	function modalityLabel(profile: LlmProviderProfile): string {
		return t(modalityI18nKey(resolveProfileModality(profile)));
	}

	const draftModality = $derived(resolveProfileModality(draft));

	async function toggleMcp(enabled: boolean) {
		try {
			await setAgentGatewayEnabled(enabled);
		} catch (caught) {
			console.error('[agent]', caught);
			await refreshAgentGateway();
		}
	}

	async function loadRuntimeSettings() {
		if (!isTauriRuntime()) return;
		try {
			const settings = await api.readRuntimeSettings();
			fileLoggingEnabled = settings.fileLoggingEnabled;
			runtimeLogError = null;
		} catch (caught) {
			console.error('[runtime-log]', caught);
		}
	}

	async function toggleRuntimeLog(enabled: boolean) {
		if (!isTauriRuntime()) return;
		runtimeLogBusy = true;
		runtimeLogError = null;
		const previous = fileLoggingEnabled;
		fileLoggingEnabled = enabled;
		try {
			const settings = await api.writeRuntimeSettings({ fileLoggingEnabled: enabled });
			fileLoggingEnabled = settings.fileLoggingEnabled;
		} catch (caught) {
			console.error('[runtime-log]', caught);
			fileLoggingEnabled = previous;
			runtimeLogError = t('settings.runtimeLog.saveFailed');
		} finally {
			runtimeLogBusy = false;
		}
	}

	onMount(() => {
		void refreshAgentGateway();
		void loadRuntimeSettings();
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
				if (tab === 'llm') save();
			}}
		>
			<header>
				<span id="settings-title">{t('settings.title')}</span>
				<button class="btn" type="button" onclick={close}>{t('settings.close')}</button>
			</header>
			<div class="settings-tabs" role="tablist" aria-label={t('settings.title')}>
				<button
					class="settings-tab"
					class:active={tab === 'basic'}
					type="button"
					role="tab"
					aria-selected={tab === 'basic'}
					onclick={() => (tab = 'basic')}
				>
					{t('settings.tab.basic')}
				</button>
				<button
					class="settings-tab"
					class:active={tab === 'llm'}
					type="button"
					role="tab"
					aria-selected={tab === 'llm'}
					onclick={() => (tab = 'llm')}
				>
					{t('settings.tab.llm')}
				</button>
			</div>
			<div class="body">
				{#if tab === 'basic'}
					<section class="mcp-settings">
						<div class="mcp-settings-copy">
							<strong>{t('settings.mcp.title')}</strong>
							<p class="hint">{t('settings.mcp.hint')}</p>
							{#if isTauriRuntime()}
								<p class="hint">
									{#if agentGateway.status?.running}
										{t('agent.port', { port: agentGateway.status.port })}
									{:else}
										{t('agent.starting')}
									{/if}
								</p>
							{:else}
								<p class="hint">{t('settings.mcp.desktopOnly')}</p>
							{/if}
						</div>
						<Switch
							name={t('settings.mcp.title')}
							checked={agentGateway.status?.enabled ?? false}
							disabled={!isTauriRuntime() || agentGateway.toggling || !agentGateway.status}
							onText={t('settings.mcp.on')}
							offText={t('settings.mcp.off')}
							onToggle={toggleMcp}
						/>
					</section>
					<section class="mcp-settings">
						<div class="mcp-settings-copy">
							<strong>{t('settings.runtimeLog.title')}</strong>
							<p class="hint">{t('settings.runtimeLog.hint')}</p>
							{#if !isTauriRuntime()}
								<p class="hint">{t('settings.runtimeLog.desktopOnly')}</p>
							{/if}
							{#if runtimeLogError}
								<p class="settings-error">{runtimeLogError}</p>
							{/if}
						</div>
						<Switch
							name={t('settings.runtimeLog.title')}
							checked={fileLoggingEnabled}
							disabled={!isTauriRuntime() || runtimeLogBusy}
							onText={t('settings.runtimeLog.on')}
							offText={t('settings.runtimeLog.off')}
							onToggle={toggleRuntimeLog}
						/>
					</section>
				{:else}
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
										<small
											>{kindLabel(provider.kind)} · {provider.model} · {modalityLabel(
												provider
											)}</small
										>
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
								<span>{t('settings.modality')}</span>
								<select
									value={draft.modality}
									onchange={(event) =>
										setModality(
											(event.currentTarget as HTMLSelectElement).value as SelectableModelModality
										)}
								>
									<option value="text">{t('agentMode.composer.modalityText')}</option>
									<option value="multimodal">{t('agentMode.composer.modalityMultimodal')}</option>
								</select>
							</label>
							<p class="hint settings-modality-hint" title={t('settings.modalityHint')}>
								{t('settings.modalityHint')}
								<span
									class="agent-composer-modality"
									class:text={draftModality === 'text'}
									class:multimodal={draftModality === 'multimodal'}
									class:unknown={draftModality === 'unknown'}
								>
									{t(modalityI18nKey(draftModality))}
								</span>
							</p>
							<label class="field">
								<span>{t('settings.contextWindow')}</span>
								<input
									type="number"
									min="1024"
									max="2000000"
									step="1024"
									bind:value={draft.contextWindow}
									autocomplete="off"
								/>
							</label>
							<p class="hint">
								{t('settings.contextWindowHint', {
									default: formatTokenCount(DEFAULT_CONTEXT_WINDOW)
								})}
							</p>
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
				{/if}
			</div>
			<footer>
				{#if tab === 'llm'}
					<span class="hint">{t('settings.keyLocal')}</span>
					{#if editingId}
						<button class="btn danger" type="button" onclick={remove}>{t('settings.delete')}</button>
					{/if}
					<button class="btn" type="button" onclick={close}>{t('settings.close')}</button>
					<button class="btn primary" type="submit">{t('settings.save')}</button>
				{:else}
					<span class="hint"></span>
					<button class="btn" type="button" onclick={close}>{t('settings.close')}</button>
				{/if}
			</footer>
		</form>
	</div>
</div>
