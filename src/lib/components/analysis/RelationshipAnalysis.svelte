<script lang="ts">
	import { onMount } from 'svelte';
	import { ChevronDown, ScrollText, X } from '@lucide/svelte';
	import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
	import { downloadTextFile } from '$lib/download';
	import { analysisPanel } from '$lib/analysis/panel.svelte';
	import { DocumentReadError, extractDocumentText } from '$lib/analysis/documents';
	import {
		analysisHistoryTitle,
		asRelationshipGraph,
		type AnalysisHistorySummary
	} from '$lib/analysis/history';
	import { edgeVisible } from '$lib/analysis/layout';
	import { renderRelationshipMarkdown } from '$lib/analysis/markdown';
	import { createAnalysisCaller } from '$lib/analysis/llm';
	import {
		analysisLogLevel,
		analysisLogStage,
		formatAnalysisError,
		type AnalysisLogEvent,
		type AnalysisLogLevel,
		type AnalysisStage,
		type CatalogQueryName
	} from '$lib/analysis/log';
	import { runRelationshipAnalysis } from '$lib/analysis/run';
	import { llmCatalog, llmSettingsDialog, selectLlmProvider } from '$lib/llm/catalog.svelte';
	import { toLlmSettings } from '$lib/llm/providers';
	import type {
		AnalysisWarning,
		Cardinality,
		CatalogObject,
		Confidence,
		GraphFilters,
		RelationshipEdge,
		RelationshipGraph,
		SchemaScope
	} from '$lib/analysis/types';
	import RelationshipGraphView from '$lib/components/analysis/RelationshipGraph.svelte';
	import { engineLabel } from '$lib/engine';
	import { formatDuration } from '$lib/format';
	import { getLocale, t } from '$lib/i18n/i18n.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';

	let selected = $state<Record<string, string[]>>({});
	let files = $state<Array<{ name: string; text: string }>>([]);
	let graph = $state<RelationshipGraph | null>(null);
	let filters = $state<GraphFilters>({ physical: true, inferred: true, strong: true, weak: true });
	let selectedId = $state<string | null>(null);
	let running = $state(false);
	let error = $state<string | null>(null);
	let progress = $state('');
	let logs = $state<Array<{ level: AnalysisLogLevel; text: string }>>([]);
	let logOpen = $state(false);
	let logList = $state<HTMLOListElement | null>(null);
	let stage = $state<AnalysisStage>('start');
	let maximized = $state(false);
	let history = $state<AnalysisHistorySummary[]>([]);
	let viewingId = $state<string | null>(null);
	const viewing = $derived(history.find((item) => item.id === viewingId) ?? null);
	let fileInput = $state<HTMLInputElement | null>(null);
	let addedIds = $state<string[]>([]);
	let connectionQuery = $state('');
	let pickerOpen = $state(false);
	let pickerHighlight = $state(0);
	let pickerRoot = $state<HTMLDivElement | null>(null);

	const visibleEdges = $derived(graph?.edges.filter((edge) => edgeVisible(edge, filters)) ?? []);
	const selectedNode = $derived(graph?.nodes.find((node) => node.id === selectedId) ?? null);
	const providers = $derived(llmCatalog.providers);
	const provider = $derived(providers.find((item) => item.id === llmCatalog.selectedId) ?? null);
	const addedConnections = $derived(
		addedIds.flatMap((id) => {
			const connection = workspace.connections.find((item) => item.id === id);
			return connection ? [connection] : [];
		})
	);
	const pickerOptions = $derived.by(() => {
		const query = connectionQuery.trim().toLowerCase();
		return workspace.connections.filter((connection) => {
			if (addedIds.includes(connection.id)) return false;
			if (!query) return true;
			const haystack = [
				connection.name,
				connection.host,
				String(connection.port),
				engineLabel(connection.engine)
			]
				.join(' ')
				.toLowerCase();
			return haystack.includes(query);
		});
	});

	$effect(() => {
		if (!logOpen) return;
		void logs.length;
		logList?.scrollTo({ top: logList.scrollHeight });
	});

	onMount(() => {
		void loadHistory();
		const onKey = (event: KeyboardEvent) => {
			if (event.key !== 'Escape' || event.defaultPrevented) return;
			analysisPanel.open = false;
		};
		const onPointerDown = (event: PointerEvent) => {
			if (!pickerOpen || !pickerRoot) return;
			if (event.target instanceof Node && pickerRoot.contains(event.target)) return;
			pickerOpen = false;
		};
		window.addEventListener('keydown', onKey);
		window.addEventListener('pointerdown', onPointerDown);
		return () => {
			window.removeEventListener('keydown', onKey);
			window.removeEventListener('pointerdown', onPointerDown);
		};
	});

	function schemasOf(connectionId: string) {
		return workspace.schema[connectionId]?.databases ?? [];
	}

	function connectionChecked(connectionId: string): boolean {
		const names = schemasOf(connectionId)
			.filter((item) => !item.isSystem)
			.map((item) => item.name);
		const picked = selected[connectionId] ?? [];
		if (names.length === 0) return picked.length > 0;
		return names.every((name) => picked.includes(name));
	}

	async function toggleConnection(connectionId: string, checked: boolean) {
		if (!checked) {
			selected = { ...selected, [connectionId]: [] };
			return;
		}
		const connection = workspace.connections.find((item) => item.id === connectionId);
		if (connection?.connected && schemasOf(connectionId).length === 0) {
			await workspace.loadDatabases(connectionId);
		}
		selected = {
			...selected,
			[connectionId]: schemasOf(connectionId)
				.filter((item) => !item.isSystem)
				.map((item) => item.name)
		};
	}

	async function addConnection(connectionId: string) {
		if (addedIds.includes(connectionId)) return;
		addedIds = [...addedIds, connectionId];
		connectionQuery = '';
		pickerOpen = false;
		pickerHighlight = 0;
		await toggleConnection(connectionId, true);
	}

	function removeConnection(connectionId: string) {
		addedIds = addedIds.filter((id) => id !== connectionId);
		const next = { ...selected };
		delete next[connectionId];
		selected = next;
	}

	function onPickerKey(event: KeyboardEvent) {
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			if (!pickerOpen) {
				pickerOpen = true;
				pickerHighlight = 0;
				return;
			}
			pickerHighlight = Math.min(Math.max(pickerOptions.length - 1, 0), pickerHighlight + 1);
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			pickerHighlight = Math.max(0, pickerHighlight - 1);
		} else if (event.key === 'Enter' && pickerOpen) {
			const option = pickerOptions[pickerHighlight];
			if (!option) return;
			event.preventDefault();
			event.stopPropagation();
			void addConnection(option.id);
		} else if (event.key === 'Escape' && pickerOpen) {
			event.preventDefault();
			event.stopPropagation();
			pickerOpen = false;
		}
	}

	function toggleSchema(connectionId: string, name: string, checked: boolean) {
		const current = new Set(selected[connectionId] ?? []);
		if (checked) current.add(name);
		else current.delete(name);
		selected = { ...selected, [connectionId]: [...current] };
	}

	function selectedScopes(): SchemaScope[] {
		const scopes: SchemaScope[] = [];
		for (const connection of workspace.connections) {
			for (const schema of selected[connection.id] ?? []) {
				scopes.push({
					connectionId: connection.id,
					connectionName: connection.name,
					engine: connection.engine,
					schema
				});
			}
		}
		return scopes;
	}

	async function onFiles(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const chosen = [...(input.files ?? [])];
		input.value = '';
		for (const file of chosen) {
			try {
				const text = await extractDocumentText(file.name, new Uint8Array(await file.arrayBuffer()));
				files = [...files, { name: file.name, text }];
			} catch (caught) {
				error = caught instanceof DocumentReadError ? fileError(caught) : errorMessage(caught);
			}
		}
	}

	function append(level: AnalysisLogLevel, text: string, next?: AnalysisStage) {
		if (next) stage = next;
		logs = [...logs, { level, text }];
		progress = text.split('\n')[0] ?? text;
		if (level === 'error') console.error(`[analysis] ${text}`);
		else console.info(`[analysis] ${text}`);
	}

	function record(event: AnalysisLogEvent) {
		append(analysisLogLevel(event), describeLog(event), analysisLogStage(event));
	}

	function describeLog(event: AnalysisLogEvent): string {
		switch (event.type) {
			case 'catalog-scope':
				return t('analysis.log.catalog.scope', { name: event.name });
			case 'catalog-query': {
				const text = t('analysis.log.catalog.query', {
					name: event.name,
					query: queryLabel(event.query),
					rows: event.rows,
					duration: formatDuration(event.durationMs)
				});
				return event.truncated ? `${text} · ${t('analysis.log.truncated')}` : text;
			}
			case 'catalog-done':
				return t('analysis.log.catalog.done', {
					objects: event.objects,
					keys: event.foreignKeys
				});
			case 'catalog-warning':
				return event.code === 'truncated'
					? t('analysis.log.catalog.warning.truncated', { name: event.name })
					: t('analysis.log.catalog.warning.empty', { name: event.name });
			case 'comments-skip':
				return t('analysis.log.comments.skip');
			case 'inference-skip':
				return t('analysis.log.inference.skip');
			case 'comments-batch':
				return t('analysis.log.comments.batch', {
					current: event.current,
					total: event.total,
					count: event.count
				});
			case 'comments-batch-done':
				return t('analysis.log.comments.done', {
					current: event.current,
					total: event.total,
					count: event.accepted,
					duration: formatDuration(event.durationMs)
				});
			case 'comments-batch-failed':
				return t('analysis.log.comments.failed', {
					current: event.current,
					total: event.total,
					count: event.count,
					duration: formatDuration(event.durationMs),
					promptChars: event.promptChars,
					ids: event.ids.join(', '),
					detail: event.detail
				});
			case 'relations-start':
				return t('analysis.log.relations.start', {
					objects: event.objects,
					known: event.known
				});
			case 'relations-done':
				return t('analysis.log.relations.done', {
					returned: event.returned,
					kept: event.kept,
					duration: formatDuration(event.durationMs)
				});
		}
	}

	function queryLabel(query: CatalogQueryName): string {
		switch (query) {
			case 'objects':
				return t('analysis.log.query.objects');
			case 'columns':
				return t('analysis.log.query.columns');
			case 'foreignKeys':
				return t('analysis.log.query.foreignKeys');
			case 'views':
				return t('analysis.log.query.views');
		}
	}

	function stageLabel(value: AnalysisStage): string {
		switch (value) {
			case 'catalog':
				return t('analysis.log.stage.catalog');
			case 'comments':
				return t('analysis.log.stage.comments');
			case 'relations':
				return t('analysis.log.stage.relations');
			case 'done':
				return t('analysis.log.stage.done');
			case 'start':
				return t('analysis.log.stage.start');
		}
	}

	function logLevel(level: AnalysisLogLevel): string {
		if (level === 'success') return t('analysis.log.level.success');
		if (level === 'warning') return t('analysis.log.level.warning');
		if (level === 'error') return t('analysis.log.level.error');
		return t('analysis.log.level.info');
	}

	function endpointLabel(baseUrl: string): string {
		const trimmed = baseUrl.trim();
		if (!trimmed) return t('analysis.log.endpoint.official');
		try {
			return new URL(trimmed).host;
		} catch {
			return t('analysis.log.endpoint.custom');
		}
	}

	async function analyze() {
		error = null;
		logs = [];
		logOpen = true;
		stage = 'start';
		progress = '';
		const scopes = selectedScopes();
		if (scopes.length === 0) {
			error = t('analysis.needScope');
			append('error', error, 'start');
			return;
		}
		const offline = scopes.find(
			(scope) => !workspace.connections.find((item) => item.id === scope.connectionId)?.connected
		);
		if (offline) {
			error = t('analysis.offlineScope', { name: offline.connectionName });
			append('error', error, 'start');
			return;
		}
		if (!provider) {
			error = t('analysis.needProvider');
			append('error', error, 'start');
			return;
		}
		if (!provider.apiKey.trim()) {
			error = t('analysis.needKey');
			append('error', error, 'start');
			return;
		}
		if (!isTauriRuntime()) {
			error = t('analysis.needTauri');
			append('error', error, 'start');
			return;
		}
		running = true;
		append(
			'info',
			t('analysis.log.start', {
				scopes: scopes.length,
				files: files.length,
				provider: provider.name,
				model: provider.model.trim(),
				endpoint: endpointLabel(provider.baseUrl)
			}),
			'start'
		);
		try {
			graph = await runRelationshipAnalysis({
				scopes,
				documents: files,
				locale: getLocale(),
				caller: createAnalysisCaller(toLlmSettings(provider)),
				runQuery: (scope, sql) => api.executeSql(scope.connectionId, sql, scope.schema),
				onLog: record
			});
			const summary = t('analysis.summary', {
				objects: graph.nodes.length,
				edges: graph.edges.length
			});
			append('success', summary, 'done');
			selectedId = graph.nodes.find((node) => !node.external)?.id ?? graph.nodes[0]?.id ?? null;
			await rememberGraph(scopes, provider.name, provider.model.trim(), graph);
		} catch (caught) {
			const message = formatAnalysisError(caught);
			error = message;
			append('error', t('analysis.log.failed', { stage: stageLabel(stage), message }), stage);
			console.error('[analysis]', caught);
		} finally {
			running = false;
		}
	}

	function fileError(caught: DocumentReadError): string {
		if (caught.code === 'too-large') return t('analysis.file.tooLarge', { name: caught.fileName });
		if (caught.code === 'read-failed')
			return t('analysis.file.readFailed', { name: caught.fileName });
		if (caught.code === 'doc') return t('analysis.file.doc', { name: caught.fileName });
		if (caught.code === 'empty') return t('analysis.file.empty', { name: caught.fileName });
		return t('analysis.file.unsupported', { name: caught.fileName });
	}

	function warningText(warning: AnalysisWarning): string {
		if (warning.code === 'truncated') {
			return t('analysis.truncated', { name: `${warning.connectionName}.${warning.schema}` });
		}
		return t('analysis.noObjects');
	}

	function edgeReason(edge: RelationshipEdge, nodes: CatalogObject[]): string {
		if (edge.reasonCode === 'foreign-key') return t('analysis.reason.foreignKey');
		if (edge.reasonCode === 'view-reference') return t('analysis.reason.view');
		if (edge.reasonCode === 'junction') {
			const via = nodes.find((node) => node.id === edge.viaId);
			return t('analysis.reason.junction', { name: via?.name ?? '' });
		}
		return edge.reason;
	}

	function nodeName(id: string): string {
		const node = graph?.nodes.find((item) => item.id === id);
		return node ? `${node.schema}.${node.name}` : id;
	}

	function originLabel(edge: RelationshipEdge): string {
		if (edge.origin === 'view') return t('analysis.origin.view');
		if (edge.origin === 'inferred') return t('analysis.origin.inferred');
		return t('analysis.origin.physical');
	}

	function confidenceLabel(value: Confidence): string {
		if (value === 'high') return t('analysis.confidence.high');
		if (value === 'medium') return t('analysis.confidence.medium');
		return t('analysis.confidence.low');
	}

	async function loadHistory() {
		if (!isTauriRuntime()) {
			history = [];
			return;
		}
		try {
			history = await api.listAnalysisHistory();
		} catch (caught) {
			error = errorMessage(caught);
		}
	}

	async function rememberGraph(
		scopes: SchemaScope[],
		providerName: string,
		model: string,
		next: RelationshipGraph
	) {
		try {
			const saved = await api.saveAnalysisHistory({
				title: analysisHistoryTitle(scopes),
				scopes: scopes.map((scope) => `${scope.connectionName} / ${scope.schema}`),
				providerName,
				model,
				graph: next
			});
			viewingId = saved.id;
			history = [saved, ...history.filter((item) => item.id !== saved.id)];
			append('info', t('analysis.historySaved'), 'done');
		} catch (caught) {
			viewingId = null;
			append('warning', t('analysis.historyFailed', { message: errorMessage(caught) }), 'done');
		}
	}

	async function openHistory(id: string) {
		if (running) return;
		try {
			const record = await api.readAnalysisHistory(id);
			const next = asRelationshipGraph(record.graph);
			if (!next) {
				error = t('analysis.historyInvalid');
				return;
			}
			graph = next;
			viewingId = record.id;
			selectedId = next.nodes.find((node) => !node.external)?.id ?? next.nodes[0]?.id ?? null;
			error = null;
			logs = [];
			progress = t('analysis.historyViewing', { time: historyTime(record.createdAt) });
		} catch (caught) {
			error = errorMessage(caught);
		}
	}

	async function deleteHistory(id: string) {
		if (running) return;
		try {
			await api.deleteAnalysisHistory(id);
			history = history.filter((item) => item.id !== id);
			if (viewingId === id) {
				viewingId = null;
				graph = null;
				selectedId = null;
				progress = '';
			}
		} catch (caught) {
			error = errorMessage(caught);
		}
	}

	function historyTime(value: string): string {
		const date = new Date(value);
		if (Number.isNaN(date.getTime())) return value;
		return date.toLocaleString();
	}

	async function exportMarkdown() {
		if (!graph || running) return;
		const content = renderRelationshipMarkdown(graph);
		const fileName = t('analysis.exportFile');
		try {
			if (!isTauriRuntime()) {
				downloadTextFile(fileName, content, 'text/markdown');
				return;
			}
			const { save } = await import('@tauri-apps/plugin-dialog');
			const path = await save({
				title: t('analysis.exportTitle'),
				defaultPath: fileName,
				filters: [{ name: 'Markdown', extensions: ['md'] }]
			});
			if (!path) return;
			await api.writeTextFile(path, content);
		} catch (caught) {
			error = errorMessage(caught);
		}
	}

	function cardinalityText(value: Cardinality): string {
		if (value === 'one_to_one') return t('analysis.cardinality.one_to_one');
		if (value === 'one_to_many') return t('analysis.cardinality.one_to_many');
		if (value === 'many_to_one') return t('analysis.cardinality.many_to_one');
		return t('analysis.cardinality.many_to_many');
	}
</script>

<div class="modal-backdrop">
	<div class="modal analysis-panel" class:maximized role="dialog" aria-labelledby="analysis-title">
		<header>
			<span id="analysis-title">{t('analysis.title')}</span>
			<div class="analysis-header-actions">
				<button
					class="btn analysis-log-toggle"
					class:alert={!logOpen &&
						logs.some((entry) => entry.level === 'error' || entry.level === 'warning')}
					type="button"
					aria-pressed={logOpen}
					aria-label={logOpen ? t('analysis.log.hide') : t('analysis.log.show')}
					title={logOpen ? t('analysis.log.hide') : t('analysis.log.show')}
					onclick={() => (logOpen = !logOpen)}
				>
					<ScrollText size={15} />
				</button>
				<button class="btn" type="button" onclick={() => (maximized = !maximized)}>
					{maximized ? t('analysis.restore') : t('analysis.maximize')}
				</button>
				<button class="btn" type="button" onclick={() => (analysisPanel.open = false)}
					>{t('analysis.close')}</button
				>
			</div>
		</header>
		<div class="analysis-body">
			<div class="analysis-side">
				<div class="analysis-side-scroll">
					<p class="analysis-intro">{t('analysis.hint')}</p>

					<section class="analysis-section">
						<h3>{t('analysis.history')}</h3>
						{#if history.length === 0}
							<p class="analysis-note">{t('analysis.historyEmpty')}</p>
						{:else}
							<ul class="analysis-history">
								{#each history as item (item.id)}
									<li class:active={item.id === viewingId}>
										<button
											type="button"
											class="analysis-history-open"
											disabled={running}
											onclick={() => openHistory(item.id)}
										>
											<span class="analysis-history-title">{item.title}</span>
											<span class="analysis-history-meta">
												<span>{historyTime(item.createdAt)}</span>
												<span
													>{t('analysis.summary', {
														objects: item.objectCount,
														edges: item.edgeCount
													})}</span
												>
											</span>
										</button>
										<button
											type="button"
											class="analysis-history-delete"
											aria-label={t('analysis.historyDelete')}
											disabled={running}
											onclick={() => deleteHistory(item.id)}
										>
											<X size={13} />
										</button>
									</li>
								{/each}
							</ul>
						{/if}
					</section>

					<section class="analysis-section">
						<h3>{t('analysis.scope')}</h3>
						{#if workspace.connections.length === 0}
							<p class="analysis-note">{t('analysis.noConnections')}</p>
						{:else}
							<div class="combobox analysis-picker" class:open={pickerOpen} bind:this={pickerRoot}>
								<input
									bind:value={connectionQuery}
									placeholder={t('analysis.searchConnection')}
									autocomplete="off"
									spellcheck="false"
									role="combobox"
									aria-expanded={pickerOpen}
									aria-autocomplete="list"
									onfocus={() => (pickerOpen = true)}
									oninput={() => {
										pickerOpen = true;
										pickerHighlight = 0;
									}}
									onkeydown={onPickerKey}
								/>
								<span class="combobox-caret"><ChevronDown size={14} /></span>
								{#if pickerOpen}
									<div class="combobox-list" role="listbox">
										{#if pickerOptions.length === 0}
											<p class="analysis-picker-empty">
												{connectionQuery.trim()
													? t('analysis.noConnectionMatch')
													: t('analysis.allConnectionsAdded')}
											</p>
										{:else}
											{#each pickerOptions as connection, index (connection.id)}
												<button
													type="button"
													class="combobox-option"
													class:active={index === pickerHighlight}
													role="option"
													aria-selected={index === pickerHighlight}
													onpointerdown={(event) => event.preventDefault()}
													onpointerenter={() => (pickerHighlight = index)}
													onclick={() => addConnection(connection.id)}
												>
													<span class="truncate">{connection.name}</span>
													<span class="combobox-hint">
														{engineLabel(connection.engine)} · {connection.host}:{connection.port}
														{#if !connection.connected}· {t('analysis.offline')}{/if}
													</span>
												</button>
											{/each}
										{/if}
									</div>
								{/if}
							</div>
						{/if}
						{#if addedConnections.length === 0 && workspace.connections.length > 0}
							<p class="analysis-note">{t('analysis.scopeEmpty')}</p>
						{/if}
						{#each addedConnections as connection (connection.id)}
							<div class="analysis-connection">
								<div class="analysis-connection-head">
									<label>
										<input
											type="checkbox"
											checked={connectionChecked(connection.id)}
											onchange={(event) =>
												toggleConnection(
													connection.id,
													(event.currentTarget as HTMLInputElement).checked
												)}
										/>
										<span class="truncate">{connection.name}</span>
										{#if !connection.connected}<em>{t('analysis.offline')}</em>{/if}
									</label>
									<button
										type="button"
										aria-label={t('analysis.removeConnection', { name: connection.name })}
										onclick={() => removeConnection(connection.id)}
									>
										<X size={13} />
									</button>
								</div>
								{#if connection.connected}
									<button
										class="btn analysis-side-btn"
										type="button"
										disabled={workspace.isPending(`db:${connection.id}`)}
										onclick={() => workspace.loadDatabases(connection.id)}
									>
										{workspace.isPending(`db:${connection.id}`)
											? t('analysis.loading')
											: t('analysis.loadSchemas')}
									</button>
									{#each schemasOf(connection.id) as schema (schema.name)}
										<label class="analysis-schema">
											<input
												type="checkbox"
												checked={selected[connection.id]?.includes(schema.name) ?? false}
												onchange={(event) =>
													toggleSchema(
														connection.id,
														schema.name,
														(event.currentTarget as HTMLInputElement).checked
													)}
											/>
											<span class="truncate">{schema.name}</span>
											{#if schema.isSystem}<em>{t('analysis.system')}</em>{/if}
										</label>
									{/each}
								{:else}
									<button
										class="btn analysis-side-btn"
										type="button"
										onclick={() => workspace.connect(connection.id)}
									>
										{t('analysis.connect')}
									</button>
								{/if}
							</div>
						{/each}
					</section>

					<section class="analysis-section">
						<h3>{t('analysis.documents')}</h3>
						<p class="analysis-note">{t('analysis.documentsHint')}</p>
						<input
							bind:this={fileInput}
							class="analysis-file"
							type="file"
							multiple
							accept=".md,.markdown,.pdf,.doc,.docx,text/markdown,application/pdf"
							onchange={onFiles}
						/>
						<button class="btn analysis-side-btn" type="button" onclick={() => fileInput?.click()}>
							{t('analysis.addFiles')}
						</button>
						<ul class="analysis-files">
							{#each files as file, index (file.name + index)}
								<li>
									<span>{file.name}</span>
									<button
										type="button"
										onclick={() => (files = files.filter((_, item) => item !== index))}
										aria-label={t('analysis.removeFile', { name: file.name })}
									>
										<X size={13} />
									</button>
								</li>
							{/each}
						</ul>
					</section>

					<section class="analysis-section">
						<h3>{t('analysis.provider')}</h3>
						{#if providers.length === 0}
							<p class="analysis-note">{t('analysis.noProviders')}</p>
						{:else}
							<select
								class="analysis-provider-select"
								aria-label={t('analysis.provider')}
								value={provider?.id ?? ''}
								onchange={(event) =>
									selectLlmProvider((event.currentTarget as HTMLSelectElement).value)}
							>
								{#each providers as item (item.id)}
									<option value={item.id}>{item.name}</option>
								{/each}
							</select>
							{#if provider}
								<p class="analysis-note">
									{provider.kind === 'anthropic'
										? t('analysis.provider.anthropic')
										: t('analysis.provider.openai')}
									· {provider.model}
								</p>
							{/if}
						{/if}
						<button
							class="btn analysis-side-btn"
							type="button"
							onclick={() => (llmSettingsDialog.open = true)}
						>
							{t('analysis.manageProviders')}
						</button>
					</section>
				</div>
			</div>
			<div class="analysis-main">
				{#if viewing}
					<p class="hint analysis-history-viewing">
						{t('analysis.historyViewing', { time: historyTime(viewing.createdAt) })} · {viewing.title}
					</p>
				{/if}
				<div class="analysis-filters">
					<label
						><input type="checkbox" bind:checked={filters.physical} />
						{t('analysis.physical')}</label
					>
					<label
						><input type="checkbox" bind:checked={filters.inferred} />
						{t('analysis.inferred')}</label
					>
					<label
						><input type="checkbox" bind:checked={filters.strong} /> {t('analysis.strong')}</label
					>
					<label><input type="checkbox" bind:checked={filters.weak} /> {t('analysis.weak')}</label>
				</div>
				{#if graph}
					<RelationshipGraphView nodes={graph.nodes} edges={visibleEdges} bind:selectedId />
				{:else}
					<div class="analysis-empty">{t('analysis.emptyGraph')}</div>
				{/if}
				{#if graph}
					{#each graph.warnings as warning, index (`${warning.code}-${index}`)}
						<p class="hint">{warningText(warning)}</p>
					{/each}
					<div class="analysis-detail">
						{#if selectedNode}
							<h3>
								{selectedNode.schema}.{selectedNode.name}
								<span
									>{selectedNode.kind === 'view'
										? t('analysis.node.view')
										: t('analysis.node.table')}</span
								>
								{#if selectedNode.external}<em>{t('analysis.external')}</em>{/if}
							</h3>
							<p>
								{selectedNode.comment || selectedNode.inferredComment || t('analysis.comment.none')}
								{#if selectedNode.inferredComment && !selectedNode.comment}
									<em>{t('analysis.comment.inferred')}</em>
								{/if}
							</p>
							<h4>{t('analysis.columns')}</h4>
							<ul>
								{#each selectedNode.columns as column (column.name)}
									<li>
										<strong>{column.name}</strong>
										{column.dataType}
										{column.key}
										{column.comment || column.inferredComment || t('analysis.comment.none')}
										{#if column.inferredComment && !column.comment}
											<em>{t('analysis.comment.inferred')}</em>
											{#if column.inferenceConfidence}
												{confidenceLabel(column.inferenceConfidence)}
											{/if}
										{/if}
									</li>
								{/each}
							</ul>
						{/if}
						<h4>{t('analysis.links')}</h4>
						<div class="analysis-edges">
							<table>
								<thead>
									<tr>
										<th>{t('analysis.from')}</th>
										<th>{t('analysis.to')}</th>
										<th>{t('analysis.kind')}</th>
										<th>{t('analysis.cardinality')}</th>
										<th>{t('analysis.reason')}</th>
									</tr>
								</thead>
								<tbody>
									{#each visibleEdges as edge (edge.id)}
										<tr>
											<td>{nodeName(edge.fromId)} {edge.fromColumns.join(', ')}</td>
											<td>{nodeName(edge.toId)} {edge.toColumns.join(', ')}</td>
											<td
												>{originLabel(edge)} · {edge.strength === 'weak'
													? t('analysis.weak')
													: t('analysis.strong')}</td
											>
											<td>{cardinalityText(edge.cardinality)}</td>
											<td>{edgeReason(edge, graph.nodes)}</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					</div>
				{/if}
			</div>
		</div>
		{#if logOpen}
			<div
				class="query-exec-log analysis-log"
				aria-live="polite"
				aria-label={t('analysis.log.title')}
			>
				<div class="query-exec-log-title">{t('analysis.log.title')}</div>
				{#if logs.length === 0}
					<p class="analysis-log-empty">{t('analysis.log.empty')}</p>
				{:else}
					<ol class="query-exec-log-list" bind:this={logList}>
						{#each logs as entry, index (index)}
							<li class="query-exec-log-item level-{entry.level}">
								<span class="query-exec-log-level">{logLevel(entry.level)}</span>
								<span class="query-exec-log-text">{entry.text}</span>
							</li>
						{/each}
					</ol>
				{/if}
			</div>
		{/if}
		<footer>
			<span class="hint">{progress}</span>
			{#if error}<span class="analysis-error">{error}</span>{/if}
			<button class="btn" type="button" disabled={!graph || running} onclick={exportMarkdown}>
				{t('analysis.export')}
			</button>
			<button class="btn primary" type="button" disabled={running} onclick={analyze}>
				{running ? t('analysis.running') : t('analysis.run')}
			</button>
		</footer>
	</div>
</div>
