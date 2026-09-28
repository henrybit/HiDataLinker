<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/tauri';
	import type { CharsetCatalog } from '$lib/api/types';
	import { workspace } from '$lib/stores/workspace.svelte';
	import { isMssql, isOracle, isSchemaScoped } from '$lib/engine';
	import Combobox from '$lib/components/layout/Combobox.svelte';
	import {
		FALLBACK_CHARSET_CATALOG,
		charsetOptions,
		collationOptions,
		collationsForCharset
	} from '$lib/mysql-charsets';
	import { schemaNounLabel, schemaNounLower, t } from '$lib/i18n/i18n.svelte';

	const prompt = $derived(workspace.createDatabasePrompt);
	const noun = $derived(schemaNounLabel(prompt?.engine));
	const nounLower = $derived(schemaNounLower(prompt?.engine));
	const schemaScoped = $derived(isSchemaScoped(prompt?.engine));
	const mssql = $derived(isMssql(prompt?.engine));
	const oracle = $derived(isOracle(prompt?.engine));
	const pending = $derived(
		prompt ? workspace.isPending(`create-db:${prompt.connectionId}`) : false
	);

	let name = $state('');
	let charset = $state('');
	let collation = $state('');
	let catalog = $state<CharsetCatalog>(FALLBACK_CHARSET_CATALOG);
	let nameInput = $state<HTMLInputElement | undefined>(undefined);
	const canSubmit = $derived(name.trim().length > 0 && !pending);
	const charsetOpts = $derived(charsetOptions(catalog.charsets));
	const collationOpts = $derived(
		collationOptions(collationsForCharset(catalog.collations, charset))
	);

	onMount(() => {
		nameInput?.focus();
		const connectionId = prompt?.connectionId;
		if (!connectionId || schemaScoped) return;
		void api.listCharsetCatalog(connectionId).then(
			(next) => {
				if (next.charsets.length) catalog = next;
			},
			() => {
				catalog = FALLBACK_CHARSET_CATALOG;
			}
		);
	});

	function cancel() {
		workspace.createDatabasePrompt = null;
	}

	function submit(event?: SubmitEvent) {
		event?.preventDefault();
		if (!canSubmit) return;
		void workspace.confirmCreateDatabase(name, charset, collation);
	}

	function applyCharset(next: string) {
		const known = catalog.charsets.find((item) => item.name === next);
		const allowed = collationsForCharset(catalog.collations, next);
		if (!known) return;
		if (!collation.trim() || (allowed.length && !allowed.some((item) => item.name === collation))) {
			collation = known.defaultCollation ?? allowed.find((item) => item.isDefault)?.name ?? '';
		}
	}
</script>

{#if prompt}
	<div class="modal-backdrop">
		<div class="modal" role="dialog" aria-modal="true" aria-labelledby="create-database-title">
			<form onsubmit={submit}>
				<header id="create-database-title">{t('dialog.createNounTitle', { noun })}</header>
				<div class="body">
					<p>
						{#if schemaScoped && prompt.database}
							{t('dialog.createNounBodyInDb', {
								noun: nounLower,
								connection: prompt.connectionName,
								database: prompt.database
							})}
						{:else}
							{t('dialog.createNounBody', {
								noun: nounLower,
								connection: prompt.connectionName
							})}
						{/if}
					</p>
					{#if oracle}
						<p>{t('dialog.oracleCreateHint')}</p>
					{/if}
					<label class="field">
						<span>{t('dialog.name')}</span>
						<input
							bind:this={nameInput}
							bind:value={name}
							placeholder={schemaScoped
								? t('dialog.schemaNamePlaceholder')
								: t('dialog.databaseNamePlaceholder')}
							disabled={pending}
							onkeydown={(event) => {
								if (event.key === 'Escape') cancel();
							}}
						/>
					</label>
					{#if mssql}
						<div class="field">
							<span>{t('dialog.collation')}</span>
							<Combobox
								bind:value={collation}
								options={collationOptions(catalog.collations)}
								placeholder={t('dialog.charsetSearch')}
								disabled={pending}
							/>
						</div>
					{:else if !schemaScoped}
						<div class="field">
							<span>{t('dialog.charset')}</span>
							<Combobox
								bind:value={charset}
								options={charsetOpts}
								placeholder={t('dialog.charsetSearch')}
								disabled={pending}
								onPick={applyCharset}
							/>
						</div>
						<div class="field">
							<span>{t('dialog.collation')}</span>
							<Combobox
								bind:value={collation}
								options={collationOpts}
								placeholder={charset.trim()
									? t('dialog.charsetSearch')
									: t('dialog.pickCharsetFirst')}
								disabled={pending}
							/>
						</div>
					{/if}
					{#if workspace.error}
						<div class="message error">{workspace.error}</div>
					{/if}
				</div>
				<footer>
					<button class="btn" type="button" onclick={cancel} disabled={pending}
						>{t('dialog.cancel')}</button
					>
					<button class="btn primary" type="submit" disabled={!canSubmit}>
						{pending ? t('dialog.creating') : t('dialog.createNoun', { noun })}
					</button>
				</footer>
			</form>
		</div>
	</div>
{/if}
