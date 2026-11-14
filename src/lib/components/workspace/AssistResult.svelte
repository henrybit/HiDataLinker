<script lang="ts">
	import type { AssistSession } from '$lib/assist/session.svelte';
	import { t } from '$lib/i18n/i18n.svelte';

	let {
		session,
		editorHasText,
		hasSelection,
		onApply,
		onRetry
	}: {
		session: AssistSession;
		editorHasText: boolean;
		hasSelection: boolean;
		onApply: (mode: 'replace' | 'insert') => void;
		onRetry: () => void;
	} = $props();

	const outcome = $derived(session.outcome);
</script>

{#if session.phase}
	<div class="assist-result muted">{t(`assist.phase.${session.phase}`)}</div>
{:else if session.failure}
	<div class="assist-result error">
		<span
			>{t(session.failure.stage === 'catalog' ? 'assist.failure.catalog' : 'assist.failure.model', {
				message: session.failure.message
			})}</span
		>
		<button class="btn" type="button" onclick={onRetry}>{t('assist.retry')}</button>
	</div>
{:else if outcome?.kind === 'empty-catalog'}
	<div class="assist-result error">{t('assist.emptyCatalog', { schema: outcome.schema })}</div>
{:else if outcome?.kind === 'clarify'}
	<div class="assist-result warn">
		<p>{outcome.response.missing || outcome.response.summary || t('assist.clarify')}</p>
		{#if outcome.response.candidates.length}
			<p class="assist-meta">
				{t('assist.candidates')}
				{#each outcome.response.candidates as name (name)}
					<code>{name}</code>
				{/each}
			</p>
		{/if}
		<p class="assist-meta">{t('assist.notWritten')}</p>
	</div>
{:else if outcome?.kind === 'rejected'}
	<div class="assist-result error">
		<p>{t('assist.rejected')}</p>
		<p class="assist-meta">
			{#each outcome.issues as issue (issue.kind + issue.name)}
				<code>{issue.name}</code>
			{/each}
		</p>
		<p class="assist-meta">{t('assist.notWritten')}</p>
	</div>
{:else if outcome?.kind === 'draft'}
	<div class="assist-result">
		{#if outcome.response.summary}<p>{outcome.response.summary}</p>{/if}
		{#if outcome.response.tables.length}
			<p class="assist-meta">
				{t('assist.usedTables')}
				{#each outcome.response.tables as name (name)}<code>{name}</code>{/each}
			</p>
		{/if}
		{#if outcome.response.relations.length}
			<p class="assist-meta">
				{t('assist.usedRelations')}
				{#each outcome.response.relations as relation (relation)}<code>{relation}</code>{/each}
			</p>
		{/if}
		{#if outcome.response.assumptions.length}
			<ul class="assist-assumptions">
				{#each outcome.response.assumptions as assumption (assumption)}
					<li>{assumption}</li>
				{/each}
			</ul>
		{/if}
		{#if session.applied}
			<p class="assist-meta ok">{t('assist.applied')}</p>
		{:else}
			{#if editorHasText}<p class="assist-meta">{t('assist.chooseApply')}</p>{/if}
			<pre class="assist-sql">{outcome.sql}</pre>
			<div class="assist-actions">
				<button class="btn primary" type="button" onclick={() => onApply('replace')}
					>{editorHasText ? t('assist.replace') : t('assist.write')}</button
				>
				{#if editorHasText}
					<button class="btn" type="button" onclick={() => onApply('insert')}
						>{hasSelection ? t('assist.replaceSelection') : t('assist.insert')}</button
					>
				{/if}
				<button class="btn" type="button" onclick={() => session.dismiss()}
					>{t('assist.discard')}</button
				>
			</div>
		{/if}
	</div>
{/if}

<style>
	.assist-result {
		margin-left: 22px;
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: white;
		font-size: 12px;
	}

	.assist-result p {
		margin: 0 0 4px;
	}

	.assist-result p:last-child {
		margin-bottom: 0;
	}

	.assist-result.muted {
		color: var(--text-muted);
	}

	.assist-result.error {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		border-color: #fecaca;
		background: #fef2f2;
		color: var(--danger);
	}

	.assist-result.warn {
		border-color: #fde68a;
		background: #fffbeb;
		color: var(--warning);
	}

	.assist-meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px;
		color: var(--text-muted);
	}

	.assist-meta.ok {
		color: var(--success);
	}

	code {
		padding: 0 4px;
		border-radius: 4px;
		background: var(--bg-muted);
		font-family: var(--mono);
		color: var(--text);
	}

	.assist-assumptions {
		margin: 0 0 4px;
		padding-left: 18px;
		color: var(--warning);
	}

	.assist-sql {
		max-height: 160px;
		margin: 4px 0;
		padding: 6px 8px;
		overflow: auto;
		border-radius: 4px;
		background: var(--bg-muted);
		font-family: var(--mono);
		white-space: pre-wrap;
	}

	.assist-actions {
		display: flex;
		gap: 6px;
	}
</style>
