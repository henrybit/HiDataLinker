<script lang="ts">
	import { workspace } from '$lib/stores/workspace.svelte';
	import Combobox from '$lib/components/layout/Combobox.svelte';
	import { t } from '$lib/i18n/i18n.svelte';

	const prompt = $derived(workspace.switchDatabasePrompt);
	const pending = $derived(
		prompt ? workspace.isPending(`switch-db:${prompt.connectionId}`) : false
	);
	const options = $derived(
		(prompt?.databases ?? []).map((name) => ({
			value: name,
			hint: name === prompt?.current ? t('dialog.currentDatabase') : undefined
		}))
	);

	let database = $state('');
	const canSubmit = $derived(
		database.trim().length > 0 && database.trim() !== prompt?.current && !pending
	);

	function cancel() {
		workspace.switchDatabasePrompt = null;
	}

	function submit(event?: SubmitEvent) {
		event?.preventDefault();
		if (!canSubmit) return;
		void workspace.confirmSwitchDatabase(database.trim());
	}
</script>

{#if prompt}
	<div class="modal-backdrop">
		<div class="modal" role="dialog" aria-modal="true" aria-labelledby="switch-database-title">
			<form onsubmit={submit}>
				<header id="switch-database-title">{t('dialog.switchDatabaseTitle')}</header>
				<div class="body">
					<p>
						{t('dialog.switchDatabaseBody', {
							connection: prompt.connectionName,
							current: prompt.current
						})}
					</p>
					<div class="field">
						<span>{t('tree.database')}</span>
						<Combobox
							bind:value={database}
							{options}
							placeholder={t('dialog.switchDatabasePlaceholder')}
							disabled={pending}
						/>
					</div>
					<p class="hint">{t('dialog.switchDatabaseHint')}</p>
					{#if workspace.error}
						<div class="message error">{workspace.error}</div>
					{/if}
				</div>
				<footer>
					<button class="btn" type="button" onclick={cancel} disabled={pending}
						>{t('dialog.cancel')}</button
					>
					<button class="btn primary" type="submit" disabled={!canSubmit}>
						{pending ? t('dialog.switching') : t('dialog.switch')}
					</button>
				</footer>
			</form>
		</div>
	</div>
{/if}
