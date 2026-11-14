<script lang="ts">
	import type { DraftStep } from '$lib/assist/draft';
	import type { AssistSession } from '$lib/assist/session.svelte';
	import { assistExplain } from '$lib/assist/statement';
	import { t } from '$lib/i18n/i18n.svelte';

	let {
		session,
		engine,
		editorHasText,
		hasSelection,
		explainBusy,
		onApply,
		onRetry,
		onRedraft,
		onExplain
	}: {
		session: AssistSession;
		engine: string;
		editorHasText: boolean;
		hasSelection: boolean;
		explainBusy: boolean;
		onApply: (mode: 'replace' | 'insert') => void;
		onRetry: () => void;
		onRedraft: (pinned: string[]) => void;
		onExplain: (sql: string) => void;
	} = $props();

	const outcome = $derived(session.outcome);
	let pinned = $state<string[]>([]);
	let showExplain = $state(false);
	const explain = $derived(
		outcome?.kind === 'draft' && showExplain ? assistExplain(outcome.sql, engine) : null
	);

	$effect(() => {
		void outcome;
		pinned = [];
		showExplain = false;
	});

	function togglePin(name: string) {
		pinned = pinned.includes(name) ? pinned.filter((item) => item !== name) : [...pinned, name];
	}

	function issueNames(step: { issues: Array<{ name: string }> }): string {
		return step.issues.map((issue) => issue.name).join(', ');
	}

	function stepLines(step: DraftStep): string[] {
		switch (step.kind) {
			case 'catalog':
				return [
					t(step.cached ? 'assist.step.catalogCached' : 'assist.step.catalogFresh', {
						objects: step.objects,
						relations: step.relations
					})
				];
			case 'scope': {
				const tables = step.tables
					.map(({ name, reason }) =>
						t('assist.step.table', { name, reason: t(`assist.reason.${reason}`) })
					)
					.join(', ');
				const lines = [t('assist.step.scope', { tables })];
				if (step.others) lines.push(t('assist.step.others', { count: step.others }));
				if (step.relations.length) {
					lines.push(t('assist.step.relations', { relations: step.relations.join('; ') }));
				}
				return lines;
			}
			case 'check':
				return [
					step.issues.length
						? t('assist.step.checkIssues', { names: issueNames(step) })
						: t('assist.step.checkOk')
				];
			case 'recheck':
				return [
					step.issues.length
						? t('assist.step.recheckIssues', { names: issueNames(step) })
						: t('assist.step.recheckOk')
				];
		}
	}
</script>

{#if session.steps.length}
	<details class="assist-steps" open={session.busy || outcome?.kind !== 'draft'}>
		<summary>{t('assist.steps', { count: session.steps.length })}</summary>
		<ol>
			{#each session.steps.flatMap(stepLines) as line, i (i)}
				<li>{line}</li>
			{/each}
		</ol>
	</details>
{/if}

{#if session.phase}
	<div class="assist-result muted">{t(`assist.phase.${session.phase}`)}</div>
{:else if session.cancelled}
	<div class="assist-result muted">{t('assist.cancelled')}</div>
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
					<button
						class="assist-chip"
						class:on={pinned.includes(name)}
						type="button"
						aria-pressed={pinned.includes(name)}
						onclick={() => togglePin(name)}>{name}</button
					>
				{/each}
			</p>
			<div class="assist-actions">
				<button
					class="btn primary"
					type="button"
					disabled={!pinned.length}
					onclick={() => onRedraft(pinned)}>{t('assist.redraft')}</button
				>
			</div>
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
			<div class="assist-actions">
				<button class="btn" type="button" onclick={() => (showExplain = !showExplain)}
					>{showExplain ? t('assist.explainHide') : t('assist.explain')}</button
				>
			</div>
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
				<button class="btn" type="button" onclick={() => (showExplain = !showExplain)}
					>{showExplain ? t('assist.explainHide') : t('assist.explain')}</button
				>
				<button class="btn" type="button" onclick={() => session.dismiss()}
					>{t('assist.discard')}</button
				>
			</div>
		{/if}
		{#if explain}
			<p class="assist-meta">{t('assist.explainNote')}</p>
			<pre class="assist-sql">{explain.statement}</pre>
			{#if explain.runnable}
				<div class="assist-actions">
					<button
						class="btn"
						type="button"
						disabled={explainBusy}
						onclick={() => outcome.kind === 'draft' && onExplain(outcome.sql)}
						>{t('assist.explainRun')}</button
					>
				</div>
			{:else}
				<p class="assist-meta warn">{t('assist.explainBlocked')}</p>
			{/if}
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

	.assist-meta.warn {
		color: var(--warning);
	}

	.assist-steps {
		margin: 0 0 4px 22px;
		font-size: 12px;
		color: var(--text-muted);
	}

	.assist-steps summary {
		cursor: pointer;
	}

	.assist-steps ol {
		margin: 2px 0 0;
		padding-left: 20px;
	}

	.assist-chip {
		padding: 0 6px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: white;
		font-family: var(--mono);
		font-size: 12px;
		color: var(--text);
		cursor: pointer;
	}

	.assist-chip.on {
		border-color: var(--accent);
		background: var(--accent);
		color: white;
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
