<script lang="ts">
	import { t } from '$lib/i18n/i18n.svelte';

	let {
		statement,
		target,
		mode,
		onConfirm,
		onCancel
	}: {
		statement: string;
		/** Connection name and schema the statement runs against. */
		target: string;
		mode: 'run' | 'explain';
		onConfirm: () => void;
		onCancel: () => void;
	} = $props();
</script>

<svelte:window onkeydown={(event) => event.key === 'Escape' && onCancel()} />

<div class="modal-backdrop">
	<div class="modal" role="dialog" aria-modal="true" aria-labelledby="draft-run-title">
		<header id="draft-run-title">{t('assist.confirm.title')}</header>
		<div class="body">
			<p>{t(mode === 'explain' ? 'assist.confirm.explainBody' : 'assist.confirm.body')}</p>
			<p class="draft-run-target">{t('assist.confirm.target', { target })}</p>
			<pre class="draft-run-sql">{statement}</pre>
		</div>
		<footer>
			<button class="btn" type="button" onclick={onCancel}>{t('dialog.cancel')}</button>
			<button class="btn danger" type="button" onclick={onConfirm}>{t('assist.confirm.run')}</button
			>
		</footer>
	</div>
</div>

<style>
	.draft-run-target {
		color: var(--text-muted);
	}

	.draft-run-sql {
		max-height: 280px;
		margin: 8px 0 0;
		padding: 8px;
		overflow: auto;
		border-radius: 4px;
		background: var(--bg-muted);
		font-family: var(--mono);
		font-size: 12px;
		white-space: pre-wrap;
	}
</style>
