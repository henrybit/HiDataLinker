<script lang="ts">
	import { folderLabel, workspace } from '$lib/stores/workspace.svelte';
	import { t, schemaNounLabel } from '$lib/i18n/i18n.svelte';
	import type { ContextMenuAction } from '$lib/api/types';

	const menu = $derived(workspace.contextMenu);

	function run(action: ContextMenuAction) {
		if (!menu) return;
		const { connectionId, schema, folder, objectKind, objectName } = menu;
		switch (action) {
			case 'new-connection':
				workspace.openNewConnection();
				break;
			case 'edit-connection':
				if (connectionId) workspace.openEditConnection(connectionId);
				break;
			case 'delete-connection':
				if (connectionId) workspace.askDeleteConnection(connectionId);
				break;
			case 'connect':
				if (connectionId) void workspace.connect(connectionId);
				break;
			case 'disconnect':
				if (connectionId) void workspace.disconnect(connectionId);
				break;
			case 'switch-database':
				if (connectionId) void workspace.askSwitchDatabase(connectionId);
				break;
			case 'new-query':
				if (connectionId) workspace.openQuery(connectionId, schema);
				break;
			case 'create-database':
				if (connectionId) workspace.askCreateDatabase(connectionId);
				break;
			case 'delete-database':
				if (connectionId && schema) workspace.askDropDatabase(connectionId, schema);
				break;
			case 'dump-database-data':
				if (connectionId && schema) void workspace.exportDatabase(connectionId, schema, false);
				break;
			case 'dump-database-full':
				if (connectionId && schema) void workspace.exportDatabase(connectionId, schema, true);
				break;
			case 'dump-table-data':
				if (connectionId && schema && objectName) {
					void workspace.exportTableData(connectionId, schema, objectName);
				}
				break;
			case 'migrate-database':
				if (connectionId && schema) workspace.askMigrateDatabase(connectionId, schema);
				break;
			case 'run-sql-file':
				if (connectionId && schema) void workspace.runSqlFile(connectionId, schema);
				break;
			case 'refresh':
				if (connectionId) void workspace.refresh(connectionId, schema, folder, objectName);
				break;
			case 'open-data':
				if (connectionId && schema && objectName) {
					workspace.openTable(connectionId, schema, objectName, objectKind === 'view');
				}
				break;
			case 'open-structure':
				if (connectionId && schema && objectName) {
					workspace.openTable(connectionId, schema, objectName, objectKind === 'view');
				}
				break;
			case 'open-ddl':
				if (connectionId && schema && objectKind && objectName) {
					workspace.openDdl(connectionId, schema, objectKind, objectName);
				}
				break;
			case 'schema-health':
				if (connectionId && schema) workspace.openSchemaHealth(connectionId, schema);
				break;
			case 'refresh-matview':
				if (connectionId && schema && objectName) {
					void workspace.refreshMaterializedView(connectionId, schema, objectName);
				}
				break;
		}
		workspace.closeMenu();
	}

	function labelFor(action: ContextMenuAction): string {
		if (
			action === 'create-database' ||
			action === 'delete-database' ||
			action === 'dump-database-data' ||
			action === 'dump-database-full' ||
			action === 'migrate-database'
		) {
			const connection = workspace.connections.find((item) => item.id === menu?.connectionId);
			const noun = schemaNounLabel(connection?.engine);
			if (action === 'create-database') return t('menu.createDatabase', { noun });
			if (action === 'delete-database') return t('menu.deleteDatabase', { noun });
			if (action === 'migrate-database') return t('menu.migrateDatabase', { noun });
			if (action === 'dump-database-data') return t('menu.dumpDatabaseData', { noun });
			return t('menu.dumpDatabaseFull', { noun });
		}
		if (action === 'refresh') {
			if (menu?.objectName) {
				if (menu.objectKind === 'view') return t('menu.refreshView');
				if (menu.objectKind === 'table') return t('menu.refreshTable');
				return t('menu.refresh');
			}
			if (menu?.folder) return t('menu.refreshFolder', { name: folderLabel(menu.folder) });
			if (menu?.schema) {
				const connection = workspace.connections.find((item) => item.id === menu.connectionId);
				return t('menu.refreshNoun', { noun: schemaNounLabel(connection?.engine) });
			}
			return t('menu.refreshConnection');
		}
		const keys: Record<ContextMenuAction, Parameters<typeof t>[0]> = {
			'new-connection': 'menu.newConnection',
			'edit-connection': 'menu.editConnection',
			'delete-connection': 'menu.deleteConnection',
			connect: 'menu.connect',
			disconnect: 'menu.disconnect',
			'switch-database': 'menu.switchDatabase',
			'new-query': 'menu.newQuery',
			'create-database': 'menu.createDatabase',
			'delete-database': 'menu.deleteDatabase',
			'dump-database-data': 'menu.dumpDatabaseData',
			'dump-database-full': 'menu.dumpDatabaseFull',
			'dump-table-data': 'menu.dumpTableData',
			'migrate-database': 'menu.migrateDatabase',
			'run-sql-file': 'menu.runSqlFile',
			refresh: 'menu.refresh',
			'open-data': 'menu.openTable',
			'open-structure': 'menu.designTable',
			'open-ddl': 'menu.viewDdl',
			'refresh-matview': 'menu.refreshMatview',
			'schema-health': 'menu.schemaHealth'
		};
		return t(keys[action]);
	}
</script>

{#if menu}
	<div class="context-menu" style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
		{#each menu.actions as action (action)}
			<button type="button" onclick={() => run(action)}>{labelFor(action)}</button>
		{/each}
	</div>
	<button
		class="modal-backdrop"
		style="background:transparent"
		onclick={() => workspace.closeMenu()}
		aria-label={t('menu.close')}
	></button>
{/if}
