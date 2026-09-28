<script lang="ts">
	import { onMount } from 'svelte';
	import { workspace } from '$lib/stores/workspace.svelte';
	import Toolbar from './Toolbar.svelte';
	import StatusBar from './StatusBar.svelte';
	import SplitPane from './SplitPane.svelte';
	import ObjectTree from '$lib/components/tree/ObjectTree.svelte';
	import TabBar from '$lib/components/workspace/TabBar.svelte';
	import ConnectionDialog from '$lib/components/dialogs/ConnectionDialog.svelte';
	import ConfirmDialog from '$lib/components/dialogs/ConfirmDialog.svelte';
	import PasswordDialog from '$lib/components/dialogs/PasswordDialog.svelte';
	import CreateDatabaseDialog from '$lib/components/dialogs/CreateDatabaseDialog.svelte';
	import DropDatabaseDialog from '$lib/components/dialogs/DropDatabaseDialog.svelte';
	import MigrateDatabaseDialog from '$lib/components/dialogs/MigrateDatabaseDialog.svelte';
	import RelationshipAnalysis from '$lib/components/analysis/RelationshipAnalysis.svelte';
	import { analysisPanel } from '$lib/analysis/panel.svelte';
	import ContextMenu from '$lib/components/layout/ContextMenu.svelte';

	let { children } = $props();

	onMount(() => {
		void workspace.boot();
		const onKey = (event: KeyboardEvent) => {
			if (event.key === 'F5') {
				event.preventDefault();
				const selected = workspace.selection;
				if (!selected) return;
				const connection = workspace.connections.find((item) => item.id === selected.connectionId);
				if (!connection?.connected) return;
				void workspace.refresh(
					selected.connectionId,
					selected.schema,
					selected.folder,
					selected.objectName
				);
			}
			if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'n' && event.shiftKey) {
				event.preventDefault();
				workspace.openNewConnection();
			}
		};
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
	});
</script>

<div class="app-shell">
	<Toolbar />
	<SplitPane>
		{#snippet left()}
			<ObjectTree />
		{/snippet}
		{#snippet right()}
			<div class="main-pane">
				<TabBar />
				<div class="main-body">
					{@render children()}
				</div>
			</div>
		{/snippet}
	</SplitPane>
	<StatusBar />
</div>

{#if workspace.dialogOpen}
	<ConnectionDialog />
{/if}
{#if workspace.confirmDelete}
	<ConfirmDialog />
{/if}
{#if workspace.passwordPrompt}
	<PasswordDialog />
{/if}
{#if workspace.createDatabasePrompt}
	<CreateDatabaseDialog />
{/if}
{#if workspace.dropDatabasePrompt}
	<DropDatabaseDialog />
{/if}
{#if workspace.migrateDatabasePrompt}
	<MigrateDatabaseDialog />
{/if}
{#if analysisPanel.open}
	<RelationshipAnalysis />
{/if}
{#if workspace.contextMenu}
	<ContextMenu />
{/if}
