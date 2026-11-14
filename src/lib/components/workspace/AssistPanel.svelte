<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Sparkles, X } from '@lucide/svelte';
	import { assistBlocker, describeContext, type AssistContext } from '$lib/assist/context';
	import { engineLabel } from '$lib/engine';
	import { t } from '$lib/i18n/i18n.svelte';
	import { llmSettingsDialog } from '$lib/llm/catalog.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';

	let {
		context,
		hasModel,
		schemas = [],
		busy = false,
		question = $bindable(''),
		showPayload = $bindable(false),
		onSubmit,
		onCancel,
		onClose,
		onPickSchema,
		children
	}: {
		context: AssistContext;
		hasModel: boolean;
		schemas?: string[];
		busy?: boolean;
		question?: string;
		showPayload?: boolean;
		onSubmit: (question: string) => void;
		onCancel?: () => void;
		onClose: () => void;
		onPickSchema?: (schema: string) => void;
		children?: Snippet;
	} = $props();

	let input = $state<HTMLTextAreaElement | undefined>();
	const blocker = $derived(assistBlocker(context, hasModel));
	const payload = $derived(describeContext(context));
	const selectionLines = $derived(
		context.editor?.selection?.trim() ? context.editor.selection.split('\n').length : 0
	);
	const canSubmit = $derived(!blocker && !busy && Boolean(question.trim()));

	export function focus() {
		input?.focus();
		input?.select();
	}

	$effect(() => {
		input?.focus();
	});

	function submit() {
		if (!canSubmit) return;
		onSubmit(question.trim());
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
			event.preventDefault();
			submit();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			if (busy) onCancel?.();
			else onClose();
		}
	}
</script>

<div class="assist-panel" role="region" aria-label={t('assist.title')}>
	<div class="assist-input-row">
		<span class="assist-icon"><Sparkles size={14} /></span>
		<textarea
			bind:this={input}
			bind:value={question}
			rows="1"
			placeholder={t(`assist.placeholder.${context.origin}`)}
			onkeydown={onKeydown}
			disabled={busy}></textarea>
		{#if busy}
			<button class="btn" type="button" onclick={() => onCancel?.()}>{t('assist.cancel')}</button>
		{:else}
			<button class="btn primary" type="button" onclick={submit} disabled={!canSubmit}
				>{t('assist.submit')}</button
			>
		{/if}
		<button class="assist-close" type="button" onclick={onClose} aria-label={t('assist.close')}>
			<X size={14} />
		</button>
	</div>

	<div class="assist-context">
		<span class="assist-chip" title={t('assist.chip.connection')}
			>{context.connectionName} · {engineLabel(context.engine)}</span
		>
		{#if context.schema}
			<span class="assist-chip" title={t('assist.chip.schema')}>{context.schema}</span>
		{/if}
		{#if context.object}
			<span class="assist-chip" title={t('assist.chip.object')}
				>{context.object.kind === 'view' ? t('assist.chip.view') : t('assist.chip.table')}
				{context.object.name}</span
			>
		{/if}
		{#if selectionLines}
			<span class="assist-chip">{t('assist.chip.selection', { count: selectionLines })}</span>
		{:else if context.editor?.text.trim()}
			<span class="assist-chip">{t('assist.chip.editor')}</span>
		{/if}
		{#if context.grid}
			<span class="assist-chip"
				>{t('assist.chip.grid', {
					columns: context.grid.columns.length,
					rows: context.grid.rowCount
				})}</span
			>
		{/if}
		{#if context.error}
			<span class="assist-chip error">{t('assist.chip.error')}</span>
		{/if}
		<button class="assist-link" type="button" onclick={() => (showPayload = !showPayload)}
			>{showPayload ? t('assist.payload.hide') : t('assist.payload.show')}</button
		>
	</div>

	{#if showPayload}
		<div class="assist-payload">
			<p>{t('assist.payload.note')}</p>
			<pre>{payload}</pre>
		</div>
	{/if}

	{#if blocker}
		<div class="assist-blocker">
			<span>{t(`assist.blocker.${blocker}`)}</span>
			{#if blocker === 'not-connected'}
				<button
					class="btn"
					type="button"
					onclick={() => void workspace.connectAsync(context.connectionId)}
					>{t('menu.connect')}</button
				>
			{:else if blocker === 'no-schema' && schemas.length && onPickSchema}
				<select
					aria-label={t('assist.chip.schema')}
					onchange={(event) => onPickSchema((event.currentTarget as HTMLSelectElement).value)}
				>
					<option value="">{t('assist.pickSchema')}</option>
					{#each schemas as name (name)}
						<option value={name}>{name}</option>
					{/each}
				</select>
			{:else if blocker === 'no-model'}
				<button class="btn" type="button" onclick={() => (llmSettingsDialog.open = true)}
					>{t('assist.configureModel')}</button
				>
			{/if}
		</div>
	{/if}

	{@render children?.()}
</div>

<style>
	.assist-panel {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 8px;
		border-bottom: 1px solid var(--border);
		background: #f5f8ff;
	}

	.assist-input-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
	}

	.assist-icon {
		display: inline-flex;
		padding-top: 7px;
		color: var(--accent);
	}

	textarea {
		flex: 1;
		min-height: 30px;
		max-height: 120px;
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		font: inherit;
		resize: vertical;
		field-sizing: content;
	}

	textarea:focus {
		outline: none;
		border-color: var(--accent);
	}

	.assist-input-row .btn:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.assist-close {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--text-muted);
	}

	.assist-close:hover {
		background: var(--bg-hover);
	}

	.assist-context {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		padding-left: 22px;
		font-size: 12px;
	}

	.assist-chip {
		padding: 1px 8px;
		border: 1px solid var(--border);
		border-radius: 999px;
		background: white;
		color: var(--text-muted);
	}

	.assist-chip.error {
		border-color: #fecaca;
		color: var(--danger);
	}

	.assist-link {
		border: none;
		background: none;
		color: var(--accent);
		font-size: 12px;
		cursor: pointer;
	}

	.assist-payload {
		margin-left: 22px;
		padding: 6px 8px;
		border: 1px dashed var(--border);
		border-radius: 6px;
		background: white;
		font-size: 12px;
		color: var(--text-muted);
	}

	.assist-payload p {
		margin: 0 0 4px;
	}

	.assist-payload pre {
		max-height: 160px;
		margin: 0;
		overflow: auto;
		font-family: var(--mono);
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--text);
	}

	.assist-blocker {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-left: 22px;
		padding: 6px 8px;
		border-radius: 6px;
		background: #fffbeb;
		color: var(--warning);
		font-size: 12px;
	}

	.assist-blocker select {
		height: 26px;
		border: 1px solid var(--border);
		border-radius: 6px;
	}
</style>
