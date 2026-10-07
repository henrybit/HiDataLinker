import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
import { qualifyIdent } from '$lib/engine';
import { uid } from '$lib/format';
import { folderMessageKey, schemaNounLabel, t } from '$lib/i18n/i18n.svelte';
import { afterPaint, runExclusive } from '$lib/runtime/jobs';
import {
	deleteQuerySqlCache,
	exceedsQuerySqlMemoryLimit,
	readQuerySqlCache,
	writeQuerySqlCache
} from '$lib/query-sql-cache';
import { pickSqlSavePath, sqlDumpFileName, writeSqlFile } from '$lib/save-sql-file';
import { connectStatus, forgetExpandedKeys } from '$lib/sessions';
import type {
	ConnectionListItem,
	ConnectionProfile,
	ContextMenuAction,
	ContextMenuState,
	CreateDatabasePrompt,
	DatabaseInfo,
	DropDatabasePrompt,
	MigrateDatabasePrompt,
	MigrateResult,
	FolderKind,
	IndexInfo,
	ObjectKind,
	QueryResult,
	RoutineInfo,
	Tab,
	TableInfo,
	TreeSelection,
	TriggerInfo,
	ViewInfo
} from '$lib/api/types';

export interface SchemaCache {
	databases: DatabaseInfo[];
	tables: Record<string, TableInfo[]>;
	views: Record<string, ViewInfo[]>;
	indexes: Record<string, IndexInfo[]>;
	triggers: Record<string, TriggerInfo[]>;
	routines: Record<string, RoutineInfo[]>;
}

function emptyCache(): SchemaCache {
	return { databases: [], tables: {}, views: {}, indexes: {}, triggers: {}, routines: {} };
}

class WorkspaceStore {
	connections = $state<ConnectionListItem[]>([]);
	expanded = $state<Set<string>>(new Set());
	schema = $state<Record<string, SchemaCache>>({});
	selection = $state<TreeSelection | null>(null);
	tabs = $state<Tab[]>([]);
	activeTabId = $state<string | null>(null);
	status = $state(t('status.ready'));
	error = $state<string | null>(null);
	pending = $state<Set<string>>(new Set());
	dialogOpen = $state(false);
	editingConnection = $state<ConnectionProfile | null>(null);
	passwordPrompt = $state<{ id: string; name: string } | null>(null);
	confirmDelete = $state<{ id: string; name: string } | null>(null);
	createDatabasePrompt = $state<CreateDatabasePrompt | null>(null);
	dropDatabasePrompt = $state<DropDatabasePrompt | null>(null);
	migrateDatabasePrompt = $state<MigrateDatabasePrompt | null>(null);
	contextMenu = $state<ContextMenuState | null>(null);
	queryResults = $state<Record<string, QueryResult | null>>({});
	lastMessage = $state<string | null>(null);
	tableDataEpoch = $state<Record<string, number>>({});
	private sqlWriteTimers = new Map<string, ReturnType<typeof setTimeout>>();
	private sqlWriteEpoch = new Map<string, number>();
	private pendingSqlWrites = new Map<string, string>();

	get activeTab(): Tab | null {
		return this.tabs.find((tab) => tab.id === this.activeTabId) ?? null;
	}

	get busy(): boolean {
		return this.pending.size > 0;
	}

	isPending(key: string): boolean {
		return this.pending.has(key);
	}

	private begin(key: string) {
		const next = new Set(this.pending);
		next.add(key);
		this.pending = next;
	}

	private end(key: string) {
		const next = new Set(this.pending);
		next.delete(key);
		this.pending = next;
	}

	async boot() {
		if (!isTauriRuntime()) {
			this.status = t('status.runTauri');
			return;
		}
		try {
			this.connections = await api.listConnections();
			this.status = t('status.savedConnections', { count: this.connections.length });
		} catch (error) {
			this.error = errorMessage(error);
		}
	}

	openNewConnection() {
		this.editingConnection = {
			id: '',
			name: 'MySQL',
			engine: 'mysql',
			host: '127.0.0.1',
			port: 3306,
			username: 'root',
			password: '',
			database: '',
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: 'auto',
			oracleConnect: 'service',
			savePassword: true
		};
		this.dialogOpen = true;
		this.closeMenu();
	}

	openEditConnection(id: string) {
		const item = this.connections.find((c) => c.id === id);
		if (!item) return;
		this.editingConnection = {
			id: item.id,
			name: item.name,
			engine: item.engine,
			host: item.host,
			port: item.port,
			username: item.username,
			password: item.password ?? '',
			sslCa: item.sslCa ?? '',
			sslCert: item.sslCert ?? '',
			sslKey: item.sslKey ?? '',
			sslVerify: item.sslVerify === true,
			oracleVersion: item.oracleVersion || 'auto',
			oracleConnect:
				item.oracleConnect === 'sid' || (item.database ?? '').toLowerCase().startsWith('sid:')
					? 'sid'
					: 'service',
			database: (item.database ?? '').replace(/^sid:/i, ''),
			savePassword: item.savePassword
		};
		this.dialogOpen = true;
		this.closeMenu();
	}

	async saveConnection(profile: ConnectionProfile) {
		const key = 'save-connection';
		this.begin(key);
		this.error = null;
		try {
			await api.upsertConnection(profile);
			await this.boot();
			this.dialogOpen = false;
			this.status = t('status.saved', { name: profile.name });
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	askDeleteConnection(id: string) {
		const item = this.connections.find((c) => c.id === id);
		if (!item) return;
		this.confirmDelete = { id, name: item.name };
		this.closeMenu();
	}

	async confirmDeleteConnection() {
		if (!this.confirmDelete) return;
		const key = `delete:${this.confirmDelete.id}`;
		this.begin(key);
		try {
			await api.deleteConnection(this.confirmDelete.id);
			this.forgetSession(this.confirmDelete.id);
			this.tabs = this.tabs.filter((tab) => tab.connectionId !== this.confirmDelete?.id);
			if (this.selection?.connectionId === this.confirmDelete.id) this.selection = null;
			this.confirmDelete = null;
			await this.boot();
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	askCreateDatabase(id: string) {
		const item = this.connections.find((c) => c.id === id);
		if (!item?.connected) return;
		this.error = null;
		this.createDatabasePrompt = {
			connectionId: item.id,
			connectionName: item.name,
			engine: item.engine,
			database: item.database
		};
		this.closeMenu();
	}

	async confirmCreateDatabase(name: string, charset?: string, collation?: string) {
		if (!this.createDatabasePrompt) return;
		const prompt = this.createDatabasePrompt;
		const created = name.trim();
		if (!created) return;
		const key = `create-db:${prompt.connectionId}`;
		this.begin(key);
		this.error = null;
		try {
			await api.createDatabase(prompt.connectionId, created, charset, collation);
			this.createDatabasePrompt = null;
			this.expanded = new Set([...this.expanded, `conn:${prompt.connectionId}`]);
			await this.loadDatabases(prompt.connectionId);
			this.selectDatabase(prompt.connectionId, created);
			this.status = t('status.created', {
				noun: schemaNounLabel(prompt.engine),
				name: created
			});
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	askDropDatabase(connectionId: string, schema: string) {
		const item = this.connections.find((c) => c.id === connectionId);
		const database = this.schema[connectionId]?.databases.find((entry) => entry.name === schema);
		if (!item?.connected || !database || database.isSystem) return;
		this.error = null;
		this.dropDatabasePrompt = {
			connectionId: item.id,
			connectionName: item.name,
			engine: item.engine,
			name: database.name
		};
		this.closeMenu();
	}

	async confirmDropDatabase() {
		if (!this.dropDatabasePrompt) return;
		const prompt = this.dropDatabasePrompt;
		const key = `drop-db:${prompt.connectionId}:${prompt.name}`;
		this.begin(key);
		this.error = null;
		try {
			await api.dropDatabase(prompt.connectionId, prompt.name);
			this.dropDatabasePrompt = null;
			const remaining = this.tabs.filter(
				(tab) => !(tab.connectionId === prompt.connectionId && tab.schema === prompt.name)
			);
			const activeStillOpen = remaining.some((tab) => tab.id === this.activeTabId);
			this.tabs = remaining;
			if (!activeStillOpen) {
				this.activeTabId = remaining[remaining.length - 1]?.id ?? null;
			}
			if (
				this.selection?.connectionId === prompt.connectionId &&
				this.selection.schema === prompt.name
			) {
				this.selection = { connectionId: prompt.connectionId };
			}
			this.expanded = new Set(
				[...this.expanded].filter(
					(key) =>
						key !== `db:${prompt.connectionId}:${prompt.name}` &&
						!key.startsWith(`folder:${prompt.connectionId}:${prompt.name}:`)
				)
			);
			const cache = this.schema[prompt.connectionId];
			if (cache) {
				const next = { ...cache };
				next.databases = next.databases.filter((item) => item.name !== prompt.name);
				delete next.tables[prompt.name];
				delete next.views[prompt.name];
				delete next.indexes[prompt.name];
				delete next.triggers[prompt.name];
				delete next.routines[prompt.name];
				this.schema = { ...this.schema, [prompt.connectionId]: next };
			}
			await this.loadDatabases(prompt.connectionId);
			this.status = t('status.dropped', {
				noun: schemaNounLabel(prompt.engine),
				name: prompt.name
			});
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	async exportDatabase(connectionId: string, schema: string, includeSchema: boolean) {
		const item = this.connections.find((c) => c.id === connectionId);
		if (!item?.connected || !schema) return;
		this.closeMenu();
		await afterPaint();
		const fileName = sqlDumpFileName(schema, includeSchema ? '' : '_data');
		const path = await pickSqlSavePath(fileName, t('dialog.saveSqlTitle'));
		if (!path) return;
		const key = `dump-db:${connectionId}:${schema}`;
		this.begin(key);
		this.error = null;
		this.status = t('status.exporting', { name: schema });
		try {
			const sql = await api.dumpDatabase(connectionId, schema, includeSchema, true);
			const saved = await writeSqlFile(path, sql, fileName);
			this.status = t('status.exportedTo', { name: schema, path: saved });
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	async exportTableData(connectionId: string, schema: string, table: string) {
		const item = this.connections.find((c) => c.id === connectionId);
		if (!item?.connected || !schema || !table) return;
		this.closeMenu();
		await afterPaint();
		const fileName = sqlDumpFileName(table, '_data');
		const path = await pickSqlSavePath(fileName, t('dialog.saveSqlTitle'));
		if (!path) return;
		const key = `dump-table:${connectionId}:${schema}:${table}`;
		this.begin(key);
		this.error = null;
		this.status = t('status.exporting', { name: table });
		try {
			const sql = await api.dumpTable(connectionId, schema, table);
			const saved = await writeSqlFile(path, sql, fileName);
			this.status = t('status.exportedTo', { name: table, path: saved });
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	askMigrateDatabase(connectionId: string, schema: string) {
		const item = this.connections.find((c) => c.id === connectionId);
		if (!item?.connected || !schema) return;
		this.error = null;
		this.migrateDatabasePrompt = {
			connectionId: item.id,
			connectionName: item.name,
			engine: item.engine,
			name: schema
		};
		this.closeMenu();
	}

	async confirmMigrateDatabase(
		targetConnectionId: string,
		targetName: string,
		includeData: boolean
	): Promise<MigrateResult | null> {
		if (!this.migrateDatabasePrompt) return null;
		const prompt = this.migrateDatabasePrompt;
		const key = `migrate-db:${prompt.connectionId}:${prompt.name}`;
		this.begin(key);
		this.error = null;
		try {
			const result = await api.migrateDatabase(
				prompt.connectionId,
				prompt.name,
				targetConnectionId,
				targetName,
				includeData
			);
			await this.loadDatabases(targetConnectionId);
			this.expanded = new Set([...this.expanded, `conn:${targetConnectionId}`]);
			this.status = t('status.migrated', {
				noun: schemaNounLabel(prompt.engine),
				source: result.sourceName,
				target: result.targetName,
				count: result.statementCount
			});
			return result;
		} catch (error) {
			this.error = errorMessage(error);
			return null;
		} finally {
			this.end(key);
		}
	}

	activateConnection(id: string) {
		this.selectConnection(id);
		const item = this.connections.find((c) => c.id === id);
		if (!item || item.connected || this.isPending(`connect:${id}`)) return;
		this.connect(id);
	}

	connect(id: string, password?: string) {
		void this.connectAsync(id, password);
	}

	/** Establish a live session. Returns true on success. */
	async connectAsync(id: string, password?: string): Promise<boolean> {
		const key = `connect:${id}`;
		this.begin(key);
		this.error = null;
		try {
			await afterPaint();
			const result = await api.connect(id, password);
			this.passwordPrompt = null;
			const evictedIds = result.evictedIds ?? [];
			const evictedNames = evictedIds.map(
				(evictedId) => this.connections.find((item) => item.id === evictedId)?.name ?? evictedId
			);
			for (const evictedId of evictedIds) this.forgetSession(evictedId);
			await this.boot();
			this.expanded = new Set([...this.expanded, `conn:${id}`]);
			void this.loadDatabases(id);
			this.status = connectStatus(evictedNames);
			return true;
		} catch (error) {
			const message = errorMessage(error);
			const item = this.connections.find((c) => c.id === id);
			if (message.toLowerCase().includes('access denied') || !item?.password) {
				this.passwordPrompt = { id, name: item?.name ?? id };
			}
			this.error = message;
			return false;
		} finally {
			this.end(key);
		}
	}

	disconnect(id: string) {
		void this.disconnectInBackground(id);
	}

	private async disconnectInBackground(id: string) {
		const key = `disconnect:${id}`;
		this.begin(key);
		try {
			await afterPaint();
			await api.disconnect(id);
			this.forgetSession(id);
			await this.boot();
			this.status = t('status.disconnected');
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	private forgetSession(id: string) {
		if (this.schema[id]) {
			delete this.schema[id];
			this.schema = { ...this.schema };
		}
		const prefix = `${id}:`;
		const nextEpoch = { ...this.tableDataEpoch };
		let epochChanged = false;
		for (const key of Object.keys(nextEpoch)) {
			if (key.startsWith(prefix)) {
				delete nextEpoch[key];
				epochChanged = true;
			}
		}
		if (epochChanged) this.tableDataEpoch = nextEpoch;
		this.expanded = forgetExpandedKeys(this.expanded, id);
	}

	toggleExpanded(key: string) {
		const next = new Set(this.expanded);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		this.expanded = next;
	}

	selectConnection(id: string) {
		this.selection = { connectionId: id };
		const item = this.connections.find((c) => c.id === id);
		if (item?.connected) {
			this.expanded = new Set([...this.expanded, `conn:${id}`]);
			void this.loadDatabases(id);
		}
	}

	selectDatabase(connectionId: string, schema: string) {
		this.selection = { connectionId, schema };
		this.expanded = new Set([
			...this.expanded,
			`conn:${connectionId}`,
			`db:${connectionId}:${schema}`
		]);
		this.openObjectsTab(connectionId, schema, 'tables');
		void this.ensureFolder(connectionId, schema, 'tables');
	}

	selectFolder(connectionId: string, schema: string, folder: FolderKind) {
		this.selection = { connectionId, schema, folder };
		this.openObjectsTab(connectionId, schema, folder);
		void this.ensureFolder(connectionId, schema, folder);
	}

	selectObject(
		connectionId: string,
		schema: string,
		folder: FolderKind,
		objectKind: ObjectKind,
		objectName: string
	) {
		this.selection = { connectionId, schema, folder, objectKind, objectName };
	}

	openObjectsTab(connectionId: string, schema: string, folder: FolderKind) {
		const id = `objects:${connectionId}:${schema}:${folder}`;
		const existing = this.tabs.find((tab) => tab.id === id);
		if (!existing) {
			this.tabs = [
				...this.tabs,
				{
					id,
					kind: 'objects',
					title: `${schema} / ${folderLabel(folder)}`,
					connectionId,
					schema,
					objectKind: folderToKind(folder),
					folder
				}
			];
		}
		this.activeTabId = id;
	}

	openTable(connectionId: string, schema: string, name: string, view = false) {
		const id = `${view ? 'view' : 'table'}:${connectionId}:${schema}:${name}`;
		if (!this.tabs.some((tab) => tab.id === id)) {
			this.tabs = [
				...this.tabs,
				{
					id,
					kind: view ? 'view' : 'table',
					title: name,
					connectionId,
					schema,
					objectName: name,
					objectKind: view ? 'view' : 'table'
				}
			];
		}
		this.activeTabId = id;
		this.closeMenu();
	}

	openDdl(connectionId: string, schema: string, kind: ObjectKind, name: string) {
		const id = `ddl:${connectionId}:${schema}:${kind}:${name}`;
		if (!this.tabs.some((tab) => tab.id === id)) {
			this.tabs = [
				...this.tabs,
				{
					id,
					kind: 'ddl',
					title: name,
					connectionId,
					schema,
					objectName: name,
					objectKind: kind
				}
			];
		}
		this.activeTabId = id;
		this.closeMenu();
	}

	async openQuery(
		connectionId: string,
		schema?: string,
		seedSql?: string,
		options?: { title?: string; autoRun?: boolean }
	) {
		const id = uid('query');
		const connection = this.connections.find((item) => item.id === connectionId);
		const sql =
			seedSql ??
			(schema ? `SELECT * FROM ${qualifyIdent(connection?.engine, schema)}` : 'SELECT 1;');
		const sqlCached = exceedsQuerySqlMemoryLimit(sql);
		if (sqlCached) {
			try {
				await writeQuerySqlCache(id, sql);
			} catch (error) {
				this.error = errorMessage(error);
				return;
			}
		}
		this.tabs = [
			...this.tabs,
			{
				id,
				kind: 'query',
				title:
					options?.title ??
					(schema
						? t('tab.queryAt', { name: schema })
						: t('tab.queryAt', { name: connection?.name ?? t('noun.database') })),
				connectionId,
				schema,
				sql: sqlCached ? undefined : sql,
				sqlCached,
				autoRun: options?.autoRun === true
			}
		];
		this.activeTabId = id;
		this.closeMenu();
	}

	clearTabAutoRun(id: string) {
		this.tabs = this.tabs.map((tab) => (tab.id === id ? { ...tab, autoRun: false } : tab));
	}

	async runSqlFile(connectionId: string, schema: string) {
		const item = this.connections.find((c) => c.id === connectionId);
		if (!item?.connected || !schema) return;
		this.closeMenu();
		this.error = null;
		try {
			const { pickSqlFile } = await import('$lib/pick-sql-file');
			const file = await pickSqlFile();
			if (!file) return;
			const sql = file.content.trim();
			if (!sql) {
				this.error = t('query.sqlFileEmpty');
				return;
			}
			await this.openQuery(connectionId, schema, file.content, {
				title: t('tab.sqlFileAt', { file: file.name, name: schema }),
				autoRun: true
			});
			this.status = t('status.sqlFileLoaded', { file: file.name, name: schema });
		} catch (error) {
			this.error = errorMessage(error);
		}
	}

	closeTab(id: string) {
		this.bumpSqlWriteEpoch(id);
		const timer = this.sqlWriteTimers.get(id);
		if (timer) clearTimeout(timer);
		this.sqlWriteTimers.delete(id);
		this.pendingSqlWrites.delete(id);
		void deleteQuerySqlCache(id);
		const index = this.tabs.findIndex((tab) => tab.id === id);
		this.tabs = this.tabs.filter((tab) => tab.id !== id);
		delete this.queryResults[id];
		this.queryResults = { ...this.queryResults };
		if (this.activeTabId === id) {
			this.activeTabId = this.tabs[index]?.id ?? this.tabs[index - 1]?.id ?? null;
		}
	}

	async loadTabSql(id: string): Promise<string> {
		const tab = this.tabs.find((item) => item.id === id);
		if (!tab) return '';
		if (tab.sqlCached) {
			const pending = this.pendingSqlWrites.get(id);
			if (pending != null) return pending;
			return readQuerySqlCache(id);
		}
		return tab.sql ?? '';
	}

	flushTabSql(id: string, sql: string) {
		if (!this.tabs.some((tab) => tab.id === id)) return;
		if (!exceedsQuerySqlMemoryLimit(sql)) {
			this.setTabSql(id, sql);
			return;
		}
		this.patchTabSqlCache(id, true);
		this.pendingSqlWrites.set(id, sql);
		void this.flushPendingSqlWrite(id);
	}

	setTabSql(id: string, sql: string) {
		const current = this.tabs.find((tab) => tab.id === id);
		if (!current) return;
		if (exceedsQuerySqlMemoryLimit(sql)) {
			this.patchTabSqlCache(id, true);
			this.scheduleSqlCacheWrite(id, sql);
			return;
		}
		if (current.sqlCached) {
			this.bumpSqlWriteEpoch(id);
			const timer = this.sqlWriteTimers.get(id);
			if (timer) clearTimeout(timer);
			this.sqlWriteTimers.delete(id);
			this.pendingSqlWrites.delete(id);
			void deleteQuerySqlCache(id);
		}
		if (current.sql === sql && !current.sqlCached) return;
		this.tabs = this.tabs.map((tab) => (tab.id === id ? { ...tab, sql, sqlCached: false } : tab));
	}

	private patchTabSqlCache(id: string, sqlCached: boolean) {
		const current = this.tabs.find((tab) => tab.id === id);
		if (!current || (current.sqlCached === sqlCached && current.sql == null)) return;
		this.tabs = this.tabs.map((tab) =>
			tab.id === id ? { ...tab, sql: undefined, sqlCached } : tab
		);
	}

	private bumpSqlWriteEpoch(id: string) {
		const next = (this.sqlWriteEpoch.get(id) ?? 0) + 1;
		this.sqlWriteEpoch.set(id, next);
		return next;
	}

	private scheduleSqlCacheWrite(id: string, sql: string) {
		this.pendingSqlWrites.set(id, sql);
		const existing = this.sqlWriteTimers.get(id);
		if (existing) clearTimeout(existing);
		const timer = setTimeout(() => {
			this.sqlWriteTimers.delete(id);
			void this.flushPendingSqlWrite(id);
		}, 400);
		this.sqlWriteTimers.set(id, timer);
	}

	private async flushPendingSqlWrite(id: string) {
		const sql = this.pendingSqlWrites.get(id);
		if (sql == null) return;
		const timer = this.sqlWriteTimers.get(id);
		if (timer) {
			clearTimeout(timer);
			this.sqlWriteTimers.delete(id);
		}
		if (!this.tabs.some((tab) => tab.id === id)) {
			this.pendingSqlWrites.delete(id);
			return;
		}
		const epoch = this.sqlWriteEpoch.get(id) ?? 0;
		try {
			await writeQuerySqlCache(id, sql);
			if ((this.sqlWriteEpoch.get(id) ?? 0) !== epoch) {
				void deleteQuerySqlCache(id);
				return;
			}
			if (this.pendingSqlWrites.get(id) === sql) {
				this.pendingSqlWrites.delete(id);
			} else if (this.pendingSqlWrites.has(id)) {
				void this.flushPendingSqlWrite(id);
			}
		} catch (error) {
			this.error = errorMessage(error);
		}
	}

	async loadDatabases(connectionId: string) {
		const key = `db:${connectionId}`;
		this.begin(key);
		try {
			await afterPaint();
			const databases = await runExclusive(key, () => api.listDatabases(connectionId));
			if (!databases) return;
			const current = this.schema[connectionId] ?? emptyCache();
			this.schema = { ...this.schema, [connectionId]: { ...current, databases } };
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	folderJobKey(connectionId: string, schema: string, folder: FolderKind) {
		return `folder:${connectionId}:${schema}:${folder}`;
	}

	async ensureFolder(connectionId: string, schema: string, folder: FolderKind, force = false) {
		const cached = this.schema[connectionId];
		if (
			!force &&
			((folder === 'tables' && cached?.tables[schema]) ||
				(folder === 'views' && cached?.views[schema]) ||
				(folder === 'indexes' && cached?.indexes[schema]) ||
				(folder === 'triggers' && cached?.triggers[schema]) ||
				(folder === 'functions' && cached?.routines[schema]))
		) {
			return;
		}

		const key = this.folderJobKey(connectionId, schema, folder);
		this.begin(key);
		try {
			await afterPaint();
			const result = await runExclusive(key, async () => {
				switch (folder) {
					case 'tables':
						return { kind: 'tables' as const, items: await api.listTables(connectionId, schema) };
					case 'views':
						return { kind: 'views' as const, items: await api.listViews(connectionId, schema) };
					case 'indexes':
						return { kind: 'indexes' as const, items: await api.listIndexes(connectionId, schema) };
					case 'triggers':
						return {
							kind: 'triggers' as const,
							items: await api.listTriggers(connectionId, schema)
						};
					case 'functions':
						return {
							kind: 'functions' as const,
							items: await api.listRoutines(connectionId, schema)
						};
				}
			});
			if (!result) return;
			const current = this.schema[connectionId] ?? emptyCache();
			const next = { ...current };
			if (result.kind === 'tables') next.tables = { ...current.tables, [schema]: result.items };
			if (result.kind === 'views') next.views = { ...current.views, [schema]: result.items };
			if (result.kind === 'indexes') next.indexes = { ...current.indexes, [schema]: result.items };
			if (result.kind === 'triggers')
				next.triggers = { ...current.triggers, [schema]: result.items };
			if (result.kind === 'functions')
				next.routines = { ...current.routines, [schema]: result.items };
			this.schema = { ...this.schema, [connectionId]: next };
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	private tableDataKey(connectionId: string, schema: string, name: string) {
		return `${connectionId}:${schema}:${name}`;
	}

	tableDataRevision(connectionId: string, schema: string, name: string): number {
		return this.tableDataEpoch[this.tableDataKey(connectionId, schema, name)] ?? 0;
	}

	private bumpTableData(connectionId: string, schema: string, objectName: string) {
		const key = this.tableDataKey(connectionId, schema, objectName);
		this.tableDataEpoch[key] = (this.tableDataEpoch[key] ?? 0) + 1;
	}

	/** Invalidate open data previews after schema changes (ALTER TABLE, etc.). */
	notifyTableChanged(connectionId: string, schema: string, objectName: string) {
		this.bumpTableData(connectionId, schema, objectName);
	}

	refresh(connectionId: string, schema?: string, folder?: FolderKind, objectName?: string) {
		this.closeMenu();
		void this.refreshInBackground(connectionId, schema, folder, objectName);
	}

	private async refreshInBackground(
		connectionId: string,
		schema?: string,
		folder?: FolderKind,
		objectName?: string
	) {
		const key = `refresh:${connectionId}:${schema ?? ''}:${folder ?? ''}:${objectName ?? ''}`;
		this.begin(key);
		this.error = null;
		try {
			await afterPaint();
			if (schema && folder) {
				await this.ensureFolder(connectionId, schema, folder, true);
				if (this.error) return;
				if (objectName && (folder === 'tables' || folder === 'views')) {
					this.bumpTableData(connectionId, schema, objectName);
					this.status = t('status.refreshedName', { name: objectName });
				} else {
					this.status = t('status.refreshedName', { name: folderLabel(folder) });
				}
				return;
			}
			await this.loadDatabases(connectionId);
			if (this.error) return;
			if (schema) {
				const [tables, views, indexes, triggers, routines] = await Promise.all([
					api.listTables(connectionId, schema),
					api.listViews(connectionId, schema),
					api.listIndexes(connectionId, schema),
					api.listTriggers(connectionId, schema),
					api.listRoutines(connectionId, schema)
				]);
				const current = this.schema[connectionId] ?? emptyCache();
				this.schema = {
					...this.schema,
					[connectionId]: {
						...current,
						tables: { ...current.tables, [schema]: tables },
						views: { ...current.views, [schema]: views },
						indexes: { ...current.indexes, [schema]: indexes },
						triggers: { ...current.triggers, [schema]: triggers },
						routines: { ...current.routines, [schema]: routines }
					}
				};
			}
			this.status = t('status.refreshed');
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.end(key);
		}
	}

	openMenu(
		event: MouseEvent,
		payload: Omit<ContextMenuState, 'x' | 'y' | 'actions'> & { actions: ContextMenuAction[] }
	) {
		event.preventDefault();
		event.stopPropagation();
		this.contextMenu = { x: event.clientX, y: event.clientY, ...payload };
	}

	closeMenu() {
		this.contextMenu = null;
	}

	setQueryResult(tabId: string, result: QueryResult | null, message?: string) {
		this.queryResults = { ...this.queryResults, [tabId]: result };
		if (message) this.lastMessage = message;
	}
}

export function folderLabel(folder: FolderKind): string {
	return t(folderMessageKey(folder));
}

export function folderToKind(folder: FolderKind): ObjectKind {
	switch (folder) {
		case 'tables':
			return 'table';
		case 'views':
			return 'view';
		case 'indexes':
			return 'index';
		case 'triggers':
			return 'trigger';
		case 'functions':
			return 'function';
	}
}

export const workspace = new WorkspaceStore();
