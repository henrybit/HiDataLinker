<script lang="ts">
	import { workspace } from '$lib/stores/workspace.svelte';
	import { engineLabel } from '$lib/engine';
	import { MAX_OPEN_SESSIONS } from '$lib/sessions';
	import { t } from '$lib/i18n/i18n.svelte';

	const selected = $derived(workspace.selection);
	const active = $derived(
		selected ? workspace.connections.find((item) => item.id === selected.connectionId) : null
	);
	const openCount = $derived(workspace.connections.filter((item) => item.connected).length);
</script>

<footer class="status-bar">
	<span title={workspace.error ?? ''}>{workspace.error ?? workspace.lastMessage ?? workspace.status}</span>
	<span>
		{#if workspace.busy}
			{t('status.tasks', { count: workspace.pending.size })}
		{:else}
			{t('status.openSessions', { open: openCount, max: MAX_OPEN_SESSIONS })}
			{#if active}
				· {engineLabel(active.engine)}
			{/if}
		{/if}
	</span>
</footer>
