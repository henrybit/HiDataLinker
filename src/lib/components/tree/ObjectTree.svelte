<script lang="ts">
	import {
		Braces,
		ChevronDown,
		ChevronRight,
		Database,
		Eye,
		FolderClosed,
		FolderOpen,
		SquareFunction,
		KeyRound,
		Server,
		Table2,
		Zap
	} from '@lucide/svelte';
	import { workspace, folderLabel } from '$lib/stores/workspace.svelte';
	import { engineLabel, isSchemaScoped } from '$lib/engine';
	import type { FolderKind } from '$lib/api/types';
	import { t } from '$lib/i18n/i18n.svelte';

	const folders: FolderKind[] = ['tables', 'views', 'indexes', 'triggers', 'functions'];

	function isSelected(
		connectionId: string,
		schema?: string,
		folder?: FolderKind,
		objectName?: string
	) {
		const sel = workspace.selection;
		return (
			sel?.connectionId === connectionId &&
			sel?.schema === schema &&
			sel?.folder === folder &&
			sel?.objectName === objectName
		);
	}

	function folderIcon(folder: FolderKind, open: boolean) {
		if (open) return FolderOpen;
		switch (folder) {
			case 'tables':
				return Table2;
			case 'views':
				return Eye;
			case 'indexes':
				return KeyRound;
			case 'triggers':
				return Zap;
			case 'functions':
				return SquareFunction;
			default:
				return FolderClosed;
		}
	}

	function objectsFor(connectionId: string, schema: string, folder: FolderKind) {
		const cache = workspace.schema[connectionId];
		if (!cache) return [];
		if (folder === 'tables') return (cache.tables[schema] ?? []).map((item) => item.name);
		if (folder === 'views') return (cache.views[schema] ?? []).map((item) => item.name);
		if (folder === 'indexes')
			return (cache.indexes[schema] ?? []).map((item) => `${item.tableName}.${item.name}`);
		if (folder === 'triggers') return (cache.triggers[schema] ?? []).map((item) => item.name);
		return (cache.routines[schema] ?? []).map(
			(item) => `${item.routineType === 'PROCEDURE' ? 'P' : 'F'}:${item.name}`
		);
	}

	function onActivate(event: KeyboardEvent, action: () => void) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			action();
		}
	}
</script>

<div class="pane-title">
	<span>{t('tree.connections')}</span>
	<button
		class="toolbar-btn"
		style="height:22px;padding:0 6px"
		type="button"
		onclick={() => workspace.openNewConnection()}>{t('tree.new')}</button
	>
</div>
<nav class="tree">
	{#if workspace.connections.length === 0}
		<div class="empty">{t('tree.noConnections')}</div>
	{/if}
	{#each workspace.connections as connection (connection.id)}
		{@const connKey = `conn:${connection.id}`}
		{@const connOpen = workspace.expanded.has(connKey)}
		<div
			class="tree-node"
			class:selected={isSelected(connection.id)}
			style="padding-left:8px"
			onclick={() => workspace.selectConnection(connection.id)}
			ondblclick={() => workspace.activateConnection(connection.id)}
			onkeydown={(event) => onActivate(event, () => workspace.selectConnection(connection.id))}
			oncontextmenu={(event) =>
				workspace.openMenu(event, {
					connectionId: connection.id,
					actions: connection.connected
						? [
								'new-query',
								'create-database',
								'refresh',
								'disconnect',
								'edit-connection',
								'delete-connection'
							]
						: ['connect', 'edit-connection', 'delete-connection', 'new-connection']
				})}
			role="button"
			tabindex="0"
			data-selected={isSelected(connection.id)}
		>
			<button
				class="chevron"
				type="button"
				onclick={(event) => {
					event.stopPropagation();
					workspace.toggleExpanded(connKey);
					if (!connOpen && connection.connected) void workspace.loadDatabases(connection.id);
				}}
				ondblclick={(event) => event.stopPropagation()}
			>
				{#if connOpen}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
			</button>
			<span
				class="icon"
				style="color:{connection.connected ? 'var(--success)' : 'var(--text-muted)'}"
			>
				<Server size={14} />
			</span>
			<span class="truncate">{connection.name}</span>
			<span class="engine-badge">{engineLabel(connection.engine)}</span>
		</div>
		{#if connOpen && connection.connected}
			{#each workspace.schema[connection.id]?.databases ?? [] as database (database.name)}
				{@const dbKey = `db:${connection.id}:${database.name}`}
				{@const dbOpen = workspace.expanded.has(dbKey)}
				<div
					class="tree-node"
					class:selected={isSelected(connection.id, database.name)}
					style="padding-left:22px"
					onclick={() => workspace.selectDatabase(connection.id, database.name)}
					onkeydown={(event) =>
						onActivate(event, () => workspace.selectDatabase(connection.id, database.name))}
					oncontextmenu={(event) =>
						workspace.openMenu(event, {
							connectionId: connection.id,
							schema: database.name,
							actions: database.isSystem
								? [
										'new-query',
										'run-sql-file',
										'create-database',
										'dump-database-data',
										'dump-database-full',
										'migrate-database',
										'refresh'
									]
								: [
										'new-query',
										'run-sql-file',
										'create-database',
										'dump-database-data',
										'dump-database-full',
										'migrate-database',
										'delete-database',
										'refresh'
									]
						})}
					role="button"
					tabindex="0"
					data-selected={isSelected(connection.id, database.name)}
				>
					<button
						class="chevron"
						onclick={(event) => {
							event.stopPropagation();
							workspace.toggleExpanded(dbKey);
							if (!dbOpen) void workspace.ensureFolder(connection.id, database.name, 'tables');
						}}
					>
						{#if dbOpen}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
					</button>
					<span class="icon" style="color:#2563eb"><Database size={14} /></span>
					<span
						class="truncate"
						title={isSchemaScoped(connection.engine) ? t('tree.schema') : t('tree.database')}
						>{database.name}</span
					>
				</div>
				{#if dbOpen}
					{#each folders as folder (folder)}
						{@const folderKey = `folder:${connection.id}:${database.name}:${folder}`}
						{@const folderOpen = workspace.expanded.has(folderKey)}
						{@const Icon = folderIcon(folder, folderOpen)}
						<div
							class="tree-node"
							class:selected={isSelected(connection.id, database.name, folder)}
							style="padding-left:36px"
							onclick={() => workspace.selectFolder(connection.id, database.name, folder)}
							onkeydown={(event) =>
								onActivate(event, () =>
									workspace.selectFolder(connection.id, database.name, folder)
								)}
							oncontextmenu={(event) =>
								workspace.openMenu(event, {
									connectionId: connection.id,
									schema: database.name,
									folder,
									actions: ['refresh']
								})}
							role="button"
							tabindex="0"
							data-selected={isSelected(connection.id, database.name, folder)}
						>
							<button
								class="chevron"
								type="button"
								onclick={(event) => {
									event.stopPropagation();
									const opening = !folderOpen;
									workspace.toggleExpanded(folderKey);
									if (opening) void workspace.ensureFolder(connection.id, database.name, folder);
								}}
							>
								{#if folderOpen}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
							</button>
							<span class="icon" style="color:#d97706"><Icon size={14} /></span>
							<span class="truncate">{folderLabel(folder)}</span>
						</div>
						{#if folderOpen}
							{#each objectsFor(connection.id, database.name, folder) as objectName (objectName)}
								{@const kind =
									folder === 'tables'
										? 'table'
										: folder === 'views'
											? 'view'
											: folder === 'indexes'
												? 'index'
												: folder === 'triggers'
													? 'trigger'
													: objectName.startsWith('P:')
														? 'procedure'
														: 'function'}
								{@const display = objectName.replace(/^[FP]:/, '')}
								<div
									class="tree-node"
									class:selected={isSelected(connection.id, database.name, folder, display)}
									style="padding-left:54px"
									onclick={() =>
										workspace.selectObject(connection.id, database.name, folder, kind, display)}
									onkeydown={(event) =>
										onActivate(event, () =>
											workspace.selectObject(connection.id, database.name, folder, kind, display)
										)}
									ondblclick={() => {
										if (kind === 'table' || kind === 'view') {
											workspace.openTable(connection.id, database.name, display, kind === 'view');
										} else if (kind !== 'index') {
											workspace.openDdl(connection.id, database.name, kind, display);
										} else {
											workspace.openObjectsTab(connection.id, database.name, 'indexes');
										}
									}}
									oncontextmenu={(event) =>
										workspace.openMenu(event, {
											connectionId: connection.id,
											schema: database.name,
											folder,
											objectKind: kind,
											objectName: display,
											actions:
												kind === 'table'
													? ['open-data', 'open-ddl', 'dump-table-data', 'new-query', 'refresh']
													: kind === 'view'
														? ['open-data', 'open-ddl', 'new-query', 'refresh']
														: ['open-ddl', 'refresh']
										})}
									role="button"
									tabindex="0"
									data-selected={isSelected(connection.id, database.name, folder, display)}
								>
									<span class="chevron"></span>
									<span class="icon" style="color:#0f766e">
										{#if kind === 'table'}<Table2 size={13} />
										{:else if kind === 'view'}<Eye size={13} />
										{:else if kind === 'index'}<KeyRound size={13} />
										{:else if kind === 'trigger'}<Zap size={13} />
										{:else}<Braces size={13} />{/if}
									</span>
									<span class="truncate">{display}</span>
								</div>
							{/each}
						{/if}
					{/each}
				{/if}
			{/each}
		{/if}
	{/each}
</nav>
