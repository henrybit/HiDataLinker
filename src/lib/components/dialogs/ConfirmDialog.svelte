<script lang="ts">
	import { workspace } from '$lib/stores/workspace.svelte';
	import { t } from '$lib/i18n/i18n.svelte';

	const deleting = $derived(
		workspace.confirmDelete
			? workspace.isPending(`delete:${workspace.confirmDelete.id}`)
			: false
	);
</script>

<div class="modal-backdrop">
	<div class="modal" role="dialog" aria-modal="true">
		<header>{t('dialog.deleteConnectionTitle')}</header>
		<div class="body">
			<p>{t('dialog.deleteConnectionBody', { name: workspace.confirmDelete?.name ?? '' })}</p>
		</div>
		<footer>
			<button class="btn" onclick={() => (workspace.confirmDelete = null)} disabled={deleting}
				>{t('dialog.cancel')}</button
			>
			<button
				class="btn danger"
				onclick={() => void workspace.confirmDeleteConnection()}
				disabled={deleting}>{t('dialog.delete')}</button
			>
		</footer>
	</div>
</div>
