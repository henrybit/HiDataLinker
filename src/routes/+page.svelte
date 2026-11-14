<script lang="ts">
	import { workspace } from '$lib/stores/workspace.svelte';
	import {
		nextMountedTabIds,
		sameStringSet,
		shouldRenderMountedTab
	} from '$lib/runtime/tab-keepalive';
	import WelcomePanel from '$lib/components/workspace/WelcomePanel.svelte';
	import ObjectList from '$lib/components/workspace/ObjectList.svelte';
	import TableWorkspace from '$lib/components/workspace/TableWorkspace.svelte';
	import DdlViewer from '$lib/components/workspace/DdlViewer.svelte';
	import QueryEditor from '$lib/components/workspace/QueryEditor.svelte';
	import SchemaHealth from '$lib/components/workspace/SchemaHealth.svelte';

	let mountedTabIds = $state(new Set<string>());

	$effect(() => {
		const openIds = new Set(workspace.tabs.map((tab) => tab.id));
		const next = nextMountedTabIds(mountedTabIds, openIds, workspace.activeTabId);
		if (!sameStringSet(mountedTabIds, next)) {
			mountedTabIds = next;
		}
	});
</script>

{#if workspace.tabs.length === 0}
	<WelcomePanel />
{:else}
	<div class="tab-panels">
		{#each workspace.tabs as tab (tab.id)}
			{@const active = workspace.activeTabId === tab.id}
			{#if mountedTabIds.has(tab.id) && shouldRenderMountedTab(tab, active)}
				<div class="tab-panel" class:active hidden={!active} aria-hidden={!active}>
					{#if tab.kind === 'objects' && tab.schema}
						<ObjectList
							connectionId={tab.connectionId}
							schema={tab.schema}
							folder={tab.folder ?? 'tables'}
						/>
					{:else if (tab.kind === 'table' || tab.kind === 'view') && tab.schema && tab.objectName}
						<TableWorkspace
							connectionId={tab.connectionId}
							schema={tab.schema}
							name={tab.objectName}
							isView={tab.kind === 'view'}
						/>
					{:else if tab.kind === 'ddl' && tab.schema && tab.objectName && tab.objectKind}
						<DdlViewer
							connectionId={tab.connectionId}
							schema={tab.schema}
							name={tab.objectName}
							kind={tab.objectKind}
						/>
					{:else if tab.kind === 'health' && tab.schema}
						<SchemaHealth connectionId={tab.connectionId} schema={tab.schema} />
					{:else if tab.kind === 'query'}
						<QueryEditor
							tabId={tab.id}
							connectionId={tab.connectionId}
							schema={tab.schema}
							sql={tab.sql ?? ''}
							sqlCached={tab.sqlCached === true}
							autoRun={tab.autoRun === true}
							assist={tab.assist}
							{active}
						/>
					{/if}
				</div>
			{/if}
		{/each}
	</div>
{/if}
