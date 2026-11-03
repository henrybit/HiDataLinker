<script lang="ts">
	import { AtSign, ArrowUp, Paperclip, Settings2, Slash, X } from '@lucide/svelte';
	import { AGENT_SKILLS, findSkill, type AgentSkillDef } from '$lib/builtin-agents/skills/catalog';
	import { t } from '$lib/i18n/i18n.svelte';
	import {
		llmCatalog,
		llmSettingsDialog,
		selectLlmProvider,
		selectedLlmProvider
	} from '$lib/llm/catalog.svelte';
	import {
		contextUsageLevel,
		contextUsageRatio,
		DEFAULT_CONTEXT_WINDOW,
		estimateTokensFromText,
		formatTokenCount,
		normalizeContextWindow
	} from '$lib/llm/context-window';
	import { detectModelModality, modalityI18nKey } from '$lib/llm/modality';
	import type { LlmProviderProfile } from '$lib/llm/providers';
	import { agentChat } from '$lib/stores/agent-chat.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';

	export type ComposerSource = {
		id: string;
		name: string;
		engine: string;
	};

	export type ComposerSkill = {
		id: string;
		slash: string;
	};

	type ActiveMenu = 'mention' | 'skill' | null;

	let {
		placeholder,
		hint,
		disabled = false,
		seedText = '',
		clearSignal = 0,
		enableSourceMention = true,
		enableSkillSlash = true,
		enableLlmPicker = true,
		enableContextMeter = true,
		onSubmit
	}: {
		placeholder?: string;
		hint?: string;
		disabled?: boolean;
		seedText?: string;
		clearSignal?: number;
		/** When true, `@` opens a connection picker for scoped agent runs. */
		enableSourceMention?: boolean;
		/** When true, `/` opens a skill picker to constrain tools for this turn. */
		enableSkillSlash?: boolean;
		/** When true, show the catalog LLM selector above the input. */
		enableLlmPicker?: boolean;
		/** When true, show context used / max progress for the active session. */
		enableContextMeter?: boolean;
		onSubmit?: (text: string, connectionIds: string[], skillIds: string[]) => void;
	} = $props();

	let localValue = $state('');
	let inputEl = $state<HTMLInputElement | null>(null);
	let lastClear = $state(0);
	let lastSeed = $state('');
	let selected = $state<ComposerSource[]>([]);
	let selectedSkills = $state<ComposerSkill[]>([]);
	let activeMenu = $state<ActiveMenu>(null);
	let menuQuery = $state('');
	let menuStart = $state<number | null>(null);
	let highlight = $state(0);

	const sources = $derived(
		workspace.connections.map((item) => ({
			id: item.id,
			name: item.name,
			engine: item.engine
		}))
	);

	const filteredSources = $derived.by(() => {
		const q = menuQuery.trim().toLowerCase();
		const picked = new Set(selected.map((item) => item.id));
		return sources
			.filter((item) => !picked.has(item.id))
			.filter((item) => {
				if (!q) return true;
				return (
					item.name.toLowerCase().includes(q) ||
					item.engine.toLowerCase().includes(q) ||
					item.id.toLowerCase().includes(q)
				);
			})
			.slice(0, 8);
	});

	const filteredSkills = $derived.by(() => {
		const q = menuQuery.trim().toLowerCase();
		const picked = new Set(selectedSkills.map((item) => item.id));
		return AGENT_SKILLS.filter((item) => !picked.has(item.id))
			.filter((item) => {
				if (!q) return true;
				const label = skillLabel(item).toLowerCase();
				const desc = skillDesc(item).toLowerCase();
				return (
					item.slash.includes(q) ||
					item.id.includes(q) ||
					label.includes(q) ||
					desc.includes(q)
				);
			})
			.slice(0, 8);
	});

	const menuItems = $derived(activeMenu === 'skill' ? filteredSkills : filteredSources);

	$effect(() => {
		if (clearSignal !== lastClear) {
			lastClear = clearSignal;
			localValue = '';
			selected = [];
			selectedSkills = [];
			closeMenus();
		}
	});

	$effect(() => {
		if (seedText && seedText !== lastSeed) {
			lastSeed = seedText;
			localValue = seedText;
		}
	});

	$effect(() => {
		menuItems;
		highlight = 0;
	});

	function skillLabel(skill: AgentSkillDef): string {
		return t(`agentMode.skills.${skill.id}.label`);
	}

	function skillDesc(skill: AgentSkillDef): string {
		return t(`agentMode.skills.${skill.id}.desc`);
	}

	function closeMenus() {
		activeMenu = null;
		menuQuery = '';
		menuStart = null;
		highlight = 0;
	}

	function detectTriggers(value: string, caret: number) {
		const before = value.slice(0, caret);

		if (enableSkillSlash) {
			const skillMatch = before.match(/(^|[\s])\/([^\s/]*)$/);
			if (skillMatch) {
				activeMenu = 'skill';
				menuQuery = skillMatch[2] ?? '';
				menuStart = caret - (skillMatch[2]?.length ?? 0) - 1;
				return;
			}
		}

		if (enableSourceMention) {
			const mentionMatch = before.match(/@([^\s@]*)$/);
			if (mentionMatch) {
				activeMenu = 'mention';
				menuQuery = mentionMatch[1] ?? '';
				menuStart = caret - mentionMatch[0].length;
				return;
			}
		}

		closeMenus();
	}

	function onInput() {
		const caret = inputEl?.selectionStart ?? localValue.length;
		detectTriggers(localValue, caret);
	}

	function replaceTrigger(insert: string) {
		if (menuStart == null || !inputEl) {
			queueMicrotask(() => inputEl?.focus());
			return;
		}
		const caret = inputEl.selectionStart ?? localValue.length;
		const before = localValue.slice(0, menuStart);
		const after = localValue.slice(caret);
		localValue = `${before}${insert}${after}`;
		const nextCaret = before.length + insert.length;
		queueMicrotask(() => {
			inputEl?.focus();
			inputEl?.setSelectionRange(nextCaret, nextCaret);
		});
	}

	function pickSource(source: ComposerSource) {
		if (!selected.some((item) => item.id === source.id)) {
			selected = [...selected, source];
		}
		if (menuStart != null) {
			replaceTrigger(`@${source.name} `);
		} else {
			queueMicrotask(() => inputEl?.focus());
		}
		closeMenus();
	}

	function pickSkill(skill: AgentSkillDef) {
		if (!selectedSkills.some((item) => item.id === skill.id)) {
			selectedSkills = [...selectedSkills, { id: skill.id, slash: skill.slash }];
		}
		if (menuStart != null) {
			replaceTrigger(`/${skill.slash} `);
		} else {
			queueMicrotask(() => inputEl?.focus());
		}
		closeMenus();
	}

	function removeSource(id: string) {
		selected = selected.filter((item) => item.id !== id);
		queueMicrotask(() => inputEl?.focus());
	}

	function removeSkill(id: string) {
		selectedSkills = selectedSkills.filter((item) => item.id !== id);
		queueMicrotask(() => inputEl?.focus());
	}

	function openMentionPicker() {
		if (disabled || !enableSourceMention) return;
		activeMenu = 'mention';
		menuQuery = '';
		menuStart = null;
		highlight = 0;
		queueMicrotask(() => inputEl?.focus());
	}

	function openSkillPicker() {
		if (disabled || !enableSkillSlash) return;
		activeMenu = 'skill';
		menuQuery = '';
		menuStart = null;
		highlight = 0;
		queueMicrotask(() => inputEl?.focus());
	}

	function submit() {
		const text = localValue.trim();
		if (disabled || !text) return;
		onSubmit?.(
			text,
			selected.map((item) => item.id),
			selectedSkills.map((item) => item.id)
		);
		localValue = '';
		selected = [];
		selectedSkills = [];
		closeMenus();
		queueMicrotask(() => inputEl?.focus());
	}

	function onKeydown(event: KeyboardEvent) {
		const items = menuItems;
		if (activeMenu && items.length > 0) {
			if (event.key === 'ArrowDown') {
				event.preventDefault();
				highlight = (highlight + 1) % items.length;
				return;
			}
			if (event.key === 'ArrowUp') {
				event.preventDefault();
				highlight = (highlight - 1 + items.length) % items.length;
				return;
			}
			if (event.key === 'Enter' || event.key === 'Tab') {
				event.preventDefault();
				const item = items[highlight] ?? items[0];
				if (!item) return;
				if (activeMenu === 'skill') pickSkill(item as AgentSkillDef);
				else pickSource(item as ComposerSource);
				return;
			}
			if (event.key === 'Escape') {
				event.preventDefault();
				closeMenus();
				return;
			}
		}

		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			submit();
		}
	}

	const badgeText = $derived.by(() => {
		const sourceCount = selected.length;
		const skillCount = selectedSkills.length;
		if (sourceCount > 0 && skillCount > 0) {
			return t('agentMode.composer.badgeBoth', {
				sources: sourceCount,
				skills: skillCount
			});
		}
		if (skillCount > 0) {
			return t('agentMode.composer.badgeSkills', { count: skillCount });
		}
		if (sourceCount > 0) {
			return t('agentMode.composer.badgeScoped', { count: sourceCount });
		}
		return t('agentMode.composer.badge');
	});

	const hintText = $derived.by(() => {
		if (hint) return hint;
		if (enableSkillSlash && enableSourceMention) return t('agentMode.composer.hintBoth');
		if (enableSkillSlash) return t('agentMode.composer.hintSkill');
		if (enableSourceMention) return t('agentMode.composer.hintMention');
		return t('agentMode.composer.hint');
	});

	const providers = $derived(llmCatalog.providers);
	const activeProvider = $derived(selectedLlmProvider());
	const activeModality = $derived(
		activeProvider ? detectModelModality(activeProvider.model) : 'unknown'
	);
	const contextMax = $derived(
		normalizeContextWindow(activeProvider?.contextWindow ?? DEFAULT_CONTEXT_WINDOW)
	);
	const contextUsed = $derived.by(() => {
		// Touch reactive deps so the meter updates as the transcript grows.
		void agentChat.messages;
		void agentChat.lastPromptContextTokens;
		return agentChat.estimateContextTokens() + estimateTokensFromText(localValue);
	});
	const contextRatio = $derived(contextUsageRatio(contextUsed, contextMax));
	const contextLevel = $derived(contextUsageLevel(contextRatio));

	function modalityLabel(profile: LlmProviderProfile): string {
		return t(modalityI18nKey(detectModelModality(profile.model)));
	}

	function providerOptionLabel(profile: LlmProviderProfile): string {
		const model = profile.model.trim() || '—';
		return `${profile.name} · ${model} · ${modalityLabel(profile)}`;
	}

	function onPickProvider(event: Event) {
		const value = (event.currentTarget as HTMLSelectElement).value;
		if (value) selectLlmProvider(value);
	}

	function openLlmSettings() {
		llmSettingsDialog.open = true;
	}
</script>

<div class="agent-composer">
	{#if enableLlmPicker}
		<div class="agent-composer-llm">
			{#if providers.length === 0}
				<p class="agent-composer-llm-empty">
					{t('agentMode.composer.llmEmpty')}
					<button type="button" class="agent-composer-llm-link" onclick={openLlmSettings}>
						{t('agentMode.composer.llmManage')}
					</button>
				</p>
			{:else}
				<label class="agent-composer-llm-field">
					<span class="agent-composer-llm-label">{t('agentMode.composer.llm')}</span>
					<select
						class="agent-composer-llm-select"
						aria-label={t('agentMode.composer.llm')}
						disabled={disabled}
						value={activeProvider?.id ?? ''}
						onchange={onPickProvider}
					>
						{#each providers as item (item.id)}
							<option value={item.id}>{providerOptionLabel(item)}</option>
						{/each}
					</select>
				</label>
				{#if activeProvider}
					<span
						class="agent-composer-modality"
						class:text={activeModality === 'text'}
						class:multimodal={activeModality === 'multimodal'}
						class:unknown={activeModality === 'unknown'}
						title={t('agentMode.composer.modalityHint')}
					>
						{t(modalityI18nKey(activeModality))}
					</span>
					<span class="agent-composer-llm-model" title={activeProvider.model}
						>{activeProvider.model}</span
					>
				{/if}
				<button
					type="button"
					class="agent-composer-llm-manage"
					title={t('agentMode.composer.llmManage')}
					onclick={openLlmSettings}
				>
					<Settings2 size={14} />
					<span>{t('agentMode.composer.llmManage')}</span>
				</button>
			{/if}
		</div>
		{#if enableContextMeter && providers.length > 0}
			<div
				class="agent-composer-context"
				class:warn={contextLevel === 'warn'}
				class:critical={contextLevel === 'critical'}
				title={t('agentMode.composer.contextHint')}
			>
				<div class="agent-composer-context-meta">
					<span class="agent-composer-context-label">{t('agentMode.composer.context')}</span>
					<span class="agent-composer-context-values">
						{t('agentMode.composer.contextUsage', {
							used: formatTokenCount(contextUsed),
							max: formatTokenCount(contextMax)
						})}
					</span>
					<span class="agent-composer-context-pct"
						>{Math.round(contextRatio * 100)}%</span
					>
				</div>
				<div
					class="agent-composer-context-track"
					role="progressbar"
					aria-label={t('agentMode.composer.context')}
					aria-valuemin={0}
					aria-valuemax={contextMax}
					aria-valuenow={Math.min(contextUsed, contextMax)}
				>
					<div
						class="agent-composer-context-fill"
						style={`width:${Math.round(contextRatio * 100)}%`}
					></div>
				</div>
			</div>
		{/if}
	{/if}

	{#if selected.length > 0 || selectedSkills.length > 0}
		<div class="agent-composer-sources" aria-label={t('agentMode.composer.chipsLabel')}>
			{#each selectedSkills as skill (skill.id)}
				{@const def = findSkill(skill.id)}
				<span class="agent-skill-chip">
					<span class="agent-skill-chip-slash">/</span>
					<span class="agent-skill-chip-name"
						>{def ? skillLabel(def) : skill.slash}</span
					>
					<button
						type="button"
						class="agent-skill-chip-remove"
						disabled={disabled}
						aria-label={t('agentMode.composer.removeSkill', { name: skill.slash })}
						onclick={() => removeSkill(skill.id)}
					>
						<X size={12} />
					</button>
				</span>
			{/each}
			{#each selected as source (source.id)}
				<span class="agent-source-chip">
					<span class="agent-source-chip-at">@</span>
					<span class="agent-source-chip-name">{source.name}</span>
					<span class="agent-source-chip-engine">{source.engine}</span>
					<button
						type="button"
						class="agent-source-chip-remove"
						disabled={disabled}
						aria-label={t('agentMode.composer.removeSource', { name: source.name })}
						onclick={() => removeSource(source.id)}
					>
						<X size={12} />
					</button>
				</span>
			{/each}
		</div>
	{/if}

	<div class="agent-composer-shell">
		{#if activeMenu === 'mention'}
			<div class="agent-mention-menu" role="listbox" aria-label={t('agentMode.composer.mentionTitle')}>
				{#if filteredSources.length === 0}
					<p class="agent-mention-empty">
						{sources.length === 0
							? t('agentMode.composer.noSources')
							: t('agentMode.composer.mentionEmpty')}
					</p>
				{:else}
					{#each filteredSources as source, index (source.id)}
						<button
							type="button"
							class="agent-mention-item"
							class:active={index === highlight}
							role="option"
							aria-selected={index === highlight}
							onmousedown={(event) => {
								event.preventDefault();
								pickSource(source);
							}}
						>
							<span class="agent-mention-at">@</span>
							<span class="agent-mention-name">{source.name}</span>
							<span class="agent-mention-meta">{source.engine}</span>
						</button>
					{/each}
				{/if}
			</div>
		{:else if activeMenu === 'skill'}
			<div class="agent-mention-menu" role="listbox" aria-label={t('agentMode.composer.skillTitle')}>
				{#if filteredSkills.length === 0}
					<p class="agent-mention-empty">{t('agentMode.composer.skillEmpty')}</p>
				{:else}
					{#each filteredSkills as skill, index (skill.id)}
						<button
							type="button"
							class="agent-mention-item agent-skill-item"
							class:active={index === highlight}
							role="option"
							aria-selected={index === highlight}
							onmousedown={(event) => {
								event.preventDefault();
								pickSkill(skill);
							}}
						>
							<span class="agent-mention-slash">/{skill.slash}</span>
							<span class="agent-mention-name">{skillLabel(skill)}</span>
							<span class="agent-mention-meta">{skillDesc(skill)}</span>
						</button>
					{/each}
				{/if}
			</div>
		{/if}

		<div class="agent-composer-bar">
			<button
				type="button"
				class="agent-composer-icon"
				title={t('agentMode.composer.attach')}
				disabled
			>
				<Paperclip size={16} />
			</button>
			{#if enableSkillSlash}
				<button
					type="button"
					class="agent-composer-icon"
					title={t('agentMode.composer.skill')}
					disabled={disabled}
					onclick={openSkillPicker}
				>
					<Slash size={16} />
				</button>
			{/if}
			{#if enableSourceMention}
				<button
					type="button"
					class="agent-composer-icon"
					title={t('agentMode.composer.mention')}
					disabled={disabled}
					onclick={openMentionPicker}
				>
					<AtSign size={16} />
				</button>
			{/if}
			<input
				bind:this={inputEl}
				class="agent-composer-input"
				type="text"
				placeholder={placeholder ?? t('agentMode.composer.placeholder')}
				bind:value={localValue}
				{disabled}
				oninput={onInput}
				onkeydown={onKeydown}
				onclick={onInput}
			/>
			<span class="agent-composer-badge">{badgeText}</span>
			<button
				type="button"
				class="agent-composer-send"
				disabled={disabled || !localValue.trim()}
				onclick={submit}
				aria-label={t('agentMode.composer.send')}
			>
				<ArrowUp size={16} />
			</button>
		</div>
	</div>
	<p class="agent-composer-hint">{hintText}</p>
</div>
