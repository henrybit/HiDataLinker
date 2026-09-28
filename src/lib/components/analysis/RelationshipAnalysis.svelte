<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
	import { analysisPanel } from '$lib/analysis/panel.svelte';
	import { DocumentReadError, extractDocumentText } from '$lib/analysis/documents';
	import { edgeVisible } from '$lib/analysis/layout';
	import { createAnalysisCaller, DEFAULT_MODELS, type LlmProvider } from '$lib/analysis/llm';
	import { runRelationshipAnalysis } from '$lib/analysis/run';
	import { loadLlmSettings, saveLlmSettings } from '$lib/analysis/settings';
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
	import { getLocale, t } from '$lib/i18n/i18n.svelte';
	import { workspace } from '$lib/stores/workspace.svelte';

	let settings = $state(loadLlmSettings());
	let selected = $state<Record<string, string[]>>({});
	let files = $state<Array<{ name: string; text: string }>>([]);
	let graph = $state<RelationshipGraph | null>(null);
	let filters = $state<GraphFilters>({ physical: true, inferred: true, strong: true, weak: true });
	let selectedId = $state<string | null>(null);
	let running = $state(false);
	let error = $state<string | null>(null);
	let progress = $state('');
	let fileInput = $state<HTMLInputElement | null>(null);

	const visibleEdges = $derived(graph?.edges.filter((edge) => edgeVisible(edge, filters)) ?? []);
	const selectedNode = $derived(graph?.nodes.find((node) => node.id === selectedId) ?? null);

	onMount(() => {
		const onKey = (event: KeyboardEvent) => {
			if (event.key === 'Escape') analysisPanel.open = false;
		};
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
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

	function toggleSchema(connectionId: string, name: string, checked: boolean) {
		const current = new Set(selected[connectionId] ?? []);
		if (checked) current.add(name);
		else current.delete(name);
		selected = { ...selected, [connectionId]: [...current] };
	}

	function setProvider(provider: LlmProvider) {
		if (settings.model.trim() === DEFAULT_MODELS[settings.provider]) {
			settings.model = DEFAULT_MODELS[provider];
		}
		settings.provider = provider;
		saveLlmSettings(settings);
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

	async function analyze() {
		error = null;
		const scopes = selectedScopes();
		if (scopes.length === 0) {
			error = t('analysis.needScope');
			return;
		}
		const offline = scopes.find(
			(scope) => !workspace.connections.find((item) => item.id === scope.connectionId)?.connected
		);
		if (offline) {
			error = t('analysis.offlineScope', { name: offline.connectionName });
			return;
		}
		if (!settings.apiKey.trim()) {
			error = t('analysis.needKey');
			return;
		}
		if (!isTauriRuntime()) {
			error = t('analysis.needTauri');
			return;
		}
		running = true;
		saveLlmSettings(settings);
		try {
			graph = await runRelationshipAnalysis({
				scopes,
				documents: files,
				locale: getLocale(),
				caller: createAnalysisCaller(settings),
				runQuery: (scope, sql) => api.executeSql(scope.connectionId, sql, scope.schema),
				onProgress: (event) => {
					if (event.phase === 'catalog' && event.scope) {
						progress = t('analysis.progress.catalog', {
							name: `${event.scope.connectionName}.${event.scope.schema}`
						});
					} else if (event.phase === 'comments') {
						progress = t('analysis.progress.comments');
					} else {
						progress = t('analysis.progress.relations');
					}
				}
			});
			progress = t('analysis.summary', { objects: graph.nodes.length, edges: graph.edges.length });
			selectedId = graph.nodes.find((node) => !node.external)?.id ?? graph.nodes[0]?.id ?? null;
		} catch (caught) {
			error = errorMessage(caught);
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

	function cardinalityText(value: Cardinality): string {
		if (value === 'one_to_one') return t('analysis.cardinality.one_to_one');
		if (value === 'one_to_many') return t('analysis.cardinality.one_to_many');
		if (value === 'many_to_one') return t('analysis.cardinality.many_to_one');
		return t('analysis.cardinality.many_to_many');
	}
</script>

<div class="modal-backdrop">
	<div class="modal analysis-panel" role="dialog" aria-labelledby="analysis-title">
		<header>
			<span id="analysis-title">{t('analysis.title')}</span>
			<button class="btn" type="button" onclick={() => (analysisPanel.open = false)}
				>{t('analysis.close')}</button
			>
		</header>
		<div class="analysis-body">
			<div class="analysis-side">
				<p class="hint">{t('analysis.hint')}</p>
				<h3>{t('analysis.scope')}</h3>
				{#if workspace.connections.length === 0}
					<p class="hint">{t('analysis.noConnections')}</p>
				{/if}
				{#each workspace.connections as connection (connection.id)}
					<div class="analysis-connection">
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
							<span>{connection.name}</span>
							{#if !connection.connected}<em>{t('analysis.offline')}</em>{/if}
						</label>
						{#if connection.connected}
							<button
								class="btn"
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
									<span>{schema.name}</span>
									{#if schema.isSystem}<em>{t('analysis.system')}</em>{/if}
								</label>
							{/each}
						{:else}
							<button class="btn" type="button" onclick={() => workspace.connect(connection.id)}>
								{t('analysis.connect')}
							</button>
						{/if}
					</div>
				{/each}

				<h3>{t('analysis.documents')}</h3>
				<p class="hint">{t('analysis.documentsHint')}</p>
				<input
					bind:this={fileInput}
					class="analysis-file"
					type="file"
					multiple
					accept=".md,.markdown,.pdf,.doc,.docx,text/markdown,application/pdf"
					onchange={onFiles}
				/>
				<button class="btn" type="button" onclick={() => fileInput?.click()}
					>{t('analysis.addFiles')}</button
				>
				<ul class="analysis-files">
					{#each files as file, index (file.name + index)}
						<li>
							<span>{file.name}</span>
							<button
								type="button"
								onclick={() => (files = files.filter((_, item) => item !== index))}
								aria-label={t('analysis.removeFile', { name: file.name })}
							>
								×
							</button>
						</li>
					{/each}
				</ul>

				<h3>{t('analysis.provider')}</h3>
				<label class="analysis-field">
					<span>{t('analysis.provider')}</span>
					<select
						value={settings.provider}
						onchange={(event) =>
							setProvider((event.currentTarget as HTMLSelectElement).value as LlmProvider)}
					>
						<option value="openai">{t('analysis.provider.openai')}</option>
						<option value="anthropic">{t('analysis.provider.anthropic')}</option>
					</select>
				</label>
				<label class="analysis-field">
					<span>{t('analysis.apiKey')}</span>
					<input
						type="password"
						bind:value={settings.apiKey}
						autocomplete="off"
						onchange={() => saveLlmSettings(settings)}
					/>
				</label>
				<label class="analysis-field">
					<span>{t('analysis.model')}</span>
					<input bind:value={settings.model} onchange={() => saveLlmSettings(settings)} />
				</label>
				<label class="analysis-field">
					<span>{t('analysis.baseUrl')}</span>
					<input
						bind:value={settings.baseUrl}
						placeholder={settings.provider === 'anthropic'
							? 'https://api.anthropic.com'
							: 'https://api.openai.com/v1'}
						onchange={() => saveLlmSettings(settings)}
					/>
				</label>
				<p class="hint">{t('analysis.baseUrlHint')}</p>
				<p class="hint">{t('analysis.keyLocal')}</p>
			</div>
			<div class="analysis-main">
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
		<footer>
			<span class="hint">{progress}</span>
			{#if error}<span class="analysis-error">{error}</span>{/if}
			<button class="btn primary" type="button" disabled={running} onclick={analyze}>
				{running ? t('analysis.running') : t('analysis.run')}
			</button>
		</footer>
	</div>
</div>
