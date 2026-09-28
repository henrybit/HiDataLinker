<script lang="ts">
	import { workspace } from '$lib/stores/workspace.svelte';
	import { api, errorMessage, isTauriRuntime } from '$lib/api/tauri';
	import {
		ENGINE_PRESETS,
		engineLabel,
		isDefaultConnectionName,
		normalizeEngine,
		type EngineKind
	} from '$lib/engine';
	import {
		connectionUrlPlaceholder,
		looksLikeConnectionUrl,
		parseConnectionUrl
	} from '$lib/connection-url';
	import { ORACLE_VERSIONS } from '$lib/oracle-version';
	import type { ConnectionProfile } from '$lib/api/types';
	import { t } from '$lib/i18n/i18n.svelte';

	let profile = $state<ConnectionProfile>(
		workspace.editingConnection ?? {
			id: '',
			name: ENGINE_PRESETS.mysql.name,
			engine: 'mysql',
			host: '127.0.0.1',
			port: ENGINE_PRESETS.mysql.port,
			username: ENGINE_PRESETS.mysql.username,
			password: '',
			database: ENGINE_PRESETS.mysql.database,
			sslCa: '',
			sslCert: '',
			sslKey: '',
			sslVerify: false,
			oracleVersion: 'auto',
			oracleConnect: 'service',
			savePassword: true
		}
	);
	let connectionUrl = $state('');
	let urlMessage = $state<string | null>(null);
	let testing = $state(false);
	let testMessage = $state<string | null>(null);
	let testOk = $state(false);
	const engine = $derived(normalizeEngine(profile.engine));
	const certificateChoice = $derived(engine === 'mssql' || engine === 'oracle');
	const urlPlaceholder = $derived(connectionUrlPlaceholder(engine));
	const hostPlaceholder = $derived(
		engine === 'postgres'
			? t('dialog.hostPlaceholderPostgres')
			: engine === 'mssql'
				? t('dialog.hostPlaceholderMssql')
				: engine === 'oracle'
					? t('dialog.hostPlaceholderOracle')
					: t('dialog.hostPlaceholderMysql')
	);
	const databaseHint = $derived(
		engine === 'postgres'
			? t('dialog.databaseHintPostgres')
			: engine === 'mssql'
				? t('dialog.databaseHintMssql')
				: engine === 'oracle'
					? profile.oracleConnect === 'sid'
						? t('dialog.sidPlaceholder')
						: t('dialog.servicePlaceholder')
					: t('dialog.databaseHintMysql')
	);

	function applyEngine(next: EngineKind) {
		const previous = ENGINE_PRESETS[normalizeEngine(profile.engine)];
		const preset = ENGINE_PRESETS[next];
		if (profile.port === previous.port) profile.port = preset.port;
		if (profile.username === previous.username) profile.username = preset.username;
		if (!profile.database || profile.database === previous.database) {
			profile.database = preset.database;
		}
		if (isDefaultConnectionName(profile.name)) {
			profile.name = preset.name;
		}
		profile.engine = next;
		if (connectionUrl.trim()) {
			const parsed = parseConnectionUrl(connectionUrl);
			if (parsed && parsed.engine !== next) {
				connectionUrl = '';
				urlMessage = null;
			}
		}
	}

	function applyParsedUrl(raw: string, options?: { fromHost?: boolean; reportInvalid?: boolean }) {
		const parsed = parseConnectionUrl(raw);
		if (!parsed) {
			if (options?.reportInvalid && looksLikeConnectionUrl(raw)) {
				urlMessage = t('dialog.invalidUrl');
			}
			return false;
		}

		profile.engine = parsed.engine;
		profile.host = parsed.host;
		profile.port = parsed.port;
		profile.username = parsed.username;
		profile.password = parsed.password;
		profile.database = parsed.database;
		profile.sslCa = parsed.sslCa;
		profile.sslCert = parsed.sslCert;
		profile.sslKey = parsed.sslKey;
		profile.sslVerify = parsed.sslVerify;
		profile.oracleVersion = parsed.oracleVersion || 'auto';
		if (parsed.engine === 'oracle' && parsed.oracleConnect) {
			profile.oracleConnect = parsed.oracleConnect;
		}
		if (isDefaultConnectionName(profile.name)) {
			profile.name = ENGINE_PRESETS[parsed.engine].name;
		}
		connectionUrl = raw.trim();
		urlMessage = options?.fromHost ? t('dialog.urlDetected') : t('dialog.urlApplied');
		return true;
	}

	function onConnectionUrlInput() {
		urlMessage = null;
		if (!connectionUrl.trim()) return;
		applyParsedUrl(connectionUrl);
	}

	function onConnectionUrlBlur() {
		if (!connectionUrl.trim()) {
			urlMessage = null;
			return;
		}
		applyParsedUrl(connectionUrl, { reportInvalid: true });
	}

	function onHostChange() {
		if (looksLikeConnectionUrl(profile.host)) {
			applyParsedUrl(profile.host, { fromHost: true });
		}
	}

	async function test() {
		testing = true;
		testMessage = null;
		testOk = false;
		try {
			await api.testConnection({
				engine: profile.engine,
				host: profile.host,
				port: Number(profile.port) || ENGINE_PRESETS[engine].port,
				username: profile.username,
				password: profile.password,
				database: profile.database,
				sslCa: profile.sslCa,
				sslCert: profile.sslCert,
				sslKey: profile.sslKey,
				sslVerify: certificateChoice && profile.sslVerify === true,
				oracleVersion: engine === 'oracle' ? profile.oracleVersion || 'auto' : null,
				oracleConnect: engine === 'oracle' && profile.oracleConnect === 'sid' ? 'sid' : null
			});
			testOk = true;
			testMessage = t('dialog.connectionSucceeded');
		} catch (error) {
			testOk = false;
			testMessage = errorMessage(error);
		} finally {
			testing = false;
		}
	}

	function optionalPath(value?: string | null): string | null {
		const trimmed = value?.trim();
		return trimmed ? trimmed : null;
	}

	function save() {
		void workspace.saveConnection({
			...profile,
			port: Number(profile.port) || ENGINE_PRESETS[engine].port,
			engine,
			sslCa:
				certificateChoice && !profile.sslVerify ? null : optionalPath(profile.sslCa),
			sslCert: engine === 'postgres' ? optionalPath(profile.sslCert) : null,
			sslKey: engine === 'postgres' ? optionalPath(profile.sslKey) : null,
			sslVerify: certificateChoice && profile.sslVerify === true,
			oracleVersion:
				engine === 'oracle' && profile.oracleVersion && profile.oracleVersion !== 'auto'
					? profile.oracleVersion
					: null,
			oracleConnect: engine === 'oracle' && profile.oracleConnect === 'sid' ? 'sid' : null
		});
	}

	async function pickSslFile(
		field: 'sslCa' | 'sslCert' | 'sslKey',
		titleKey: 'dialog.sslCa' | 'dialog.sslCert' | 'dialog.sslKey',
		extensions: string[]
	) {
		if (!isTauriRuntime()) return;
		const { open } = await import('@tauri-apps/plugin-dialog');
		const selected = await open({
			title: t(titleKey),
			multiple: false,
			filters: [{ name: 'Certificate', extensions }]
		});
		if (typeof selected === 'string') profile[field] = selected;
	}
</script>

<div class="modal-backdrop">
	<div class="modal modal-wide" role="dialog" aria-modal="true">
		<header>{t('dialog.connectionTitle', { engine: engineLabel(engine) })}</header>
		<div class="body">
			<label class="field">
				<span>{t('dialog.engine')}</span>
				<select
					value={engine}
					onchange={(event) => applyEngine(normalizeEngine(event.currentTarget.value))}
				>
					<option value="mysql">MySQL</option>
					<option value="postgres">PostgreSQL</option>
					<option value="mssql">SQL Server</option>
					<option value="oracle">Oracle</option>
				</select>
			</label>
			<label class="field">
				<span>{t('dialog.url')}</span>
				<input
					bind:value={connectionUrl}
					placeholder={urlPlaceholder}
					oninput={onConnectionUrlInput}
					onblur={onConnectionUrlBlur}
					onpaste={() => queueMicrotask(onConnectionUrlInput)}
				/>
			</label>
			{#if urlMessage}
				<div class="message">{urlMessage}</div>
			{/if}
			<label class="field">
				<span>{t('dialog.name')}</span>
				<input bind:value={profile.name} />
			</label>
			<label class="field">
				<span>{t('dialog.host')}</span>
				<input
					bind:value={profile.host}
					placeholder={hostPlaceholder}
					oninput={onHostChange}
					onpaste={() => queueMicrotask(onHostChange)}
				/>
			</label>
			<label class="field">
				<span>{t('dialog.port')}</span>
				<input type="number" bind:value={profile.port} />
			</label>
			<label class="field">
				<span>{t('dialog.user')}</span>
				<input bind:value={profile.username} />
			</label>
			<label class="field">
				<span>{t('dialog.password')}</span>
				<input type="password" bind:value={profile.password} />
			</label>
			{#if engine === 'oracle'}
				<label class="field">
					<span>{t('dialog.oracleConnect')}</span>
					<select
						bind:value={profile.oracleConnect}
						onchange={() => {
							const value = profile.database?.trim() ?? '';
							if (profile.oracleConnect === 'sid' && value.toLowerCase().startsWith('sid:')) {
								profile.database = value.slice(4);
							}
						}}
					>
						<option value="service">{t('dialog.serviceName')}</option>
						<option value="sid">{t('dialog.sid')}</option>
					</select>
				</label>
				<p class="hint">{t('dialog.oracleConnectHint')}</p>
			{/if}
			<label class="field">
				<span
					>{engine === 'oracle'
						? profile.oracleConnect === 'sid'
							? t('dialog.sid')
							: t('dialog.serviceName')
						: t('dialog.database')}</span
				>
				<input bind:value={profile.database} placeholder={databaseHint} />
			</label>
			{#if engine === 'oracle'}
				<label class="field">
					<span>{t('dialog.oracleVersion')}</span>
					<select bind:value={profile.oracleVersion}>
						<option value="auto">{t('dialog.oracleVersionAuto')}</option>
						{#each ORACLE_VERSIONS as version (version.id)}
							<option value={version.id}>{version.label}</option>
						{/each}
					</select>
				</label>
				<p class="hint">{t('dialog.oracleVersionHint')}</p>
			{/if}
			{#if certificateChoice}
				<label class="field">
					<span>{t('dialog.verifyCert')}</span>
					<input
						type="checkbox"
						checked={profile.sslVerify === true}
						onchange={(event) => {
							profile.sslVerify = (event.currentTarget as HTMLInputElement).checked;
						}}
					/>
				</label>
				<p class="hint">
					{engine === 'oracle'
						? profile.sslVerify
							? t('dialog.verifyCertOnOracle')
							: t('dialog.verifyCertOffOracle')
						: profile.sslVerify
							? t('dialog.verifyCertOn')
							: t('dialog.verifyCertOff')}
				</p>
			{/if}
			{#if engine === 'mysql' || engine === 'postgres' || (certificateChoice && profile.sslVerify)}
				<label class="field">
					<span>{t('dialog.sslCa')}</span>
					<div class="field-control">
						<input
							bind:value={profile.sslCa}
							placeholder={t('dialog.sslCaPlaceholder')}
							spellcheck="false"
						/>
						<button
							class="btn"
							type="button"
							onclick={() =>
								void pickSslFile('sslCa', 'dialog.sslCa', ['pem', 'crt', 'cer', 'cert'])}
							>{t('dialog.browse')}</button
						>
					</div>
				</label>
			{/if}
			{#if engine === 'postgres'}
					<label class="field">
						<span>{t('dialog.sslCert')}</span>
						<div class="field-control">
							<input
								bind:value={profile.sslCert}
								placeholder={t('dialog.sslCertPlaceholder')}
								spellcheck="false"
							/>
							<button
								class="btn"
								type="button"
								onclick={() =>
									void pickSslFile('sslCert', 'dialog.sslCert', ['pem', 'crt', 'cer', 'cert'])}
								>{t('dialog.browse')}</button
							>
						</div>
					</label>
					<label class="field">
						<span>{t('dialog.sslKey')}</span>
						<div class="field-control">
							<input
								bind:value={profile.sslKey}
								placeholder={t('dialog.sslKeyPlaceholder')}
								spellcheck="false"
							/>
							<button
								class="btn"
								type="button"
								onclick={() => void pickSslFile('sslKey', 'dialog.sslKey', ['pem', 'key'])}
								>{t('dialog.browse')}</button
							>
						</div>
					</label>
			{/if}
			<label class="field">
				<span>{t('dialog.savePassword')}</span>
				<input type="checkbox" bind:checked={profile.savePassword} />
			</label>
			{#if testMessage}
				<div class="message" class:error={!testOk}>{testMessage}</div>
			{/if}
		</div>
		<footer>
			<button class="btn" onclick={test} disabled={testing}
				>{testing ? t('dialog.testing') : t('dialog.test')}</button
			>
			<button class="btn" onclick={() => (workspace.dialogOpen = false)}
				>{t('dialog.cancel')}</button
			>
			<button class="btn primary" onclick={save} disabled={workspace.isPending('save-connection')}
				>{t('dialog.save')}</button
			>
		</footer>
	</div>
</div>
