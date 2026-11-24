<script lang="ts">
	import { uid } from '$lib/format';
	import type { MessageKey } from '$lib/i18n/en';
	import { t } from '$lib/i18n/i18n.svelte';
	import {
		newFieldCodecRule,
		type AesMode,
		type ByteEncoding,
		type CipherEncoding,
		type CodecRuleProblem,
		type FieldCodecOp,
		type FieldCodecRule
	} from '$lib/sql/field-codec';

	let {
		rules,
		columns,
		issues,
		onChange
	}: {
		rules: FieldCodecRule[];
		columns: string[];
		issues: CodecRuleProblem[];
		onChange: (rules: FieldCodecRule[]) => void;
	} = $props();

	const listId = uid('codec-cols');
	let revealed = $state<Record<string, boolean>>({});

	const issueById = $derived(new Map(issues.map((issue) => [issue.id, issue])));

	function update(next: FieldCodecRule[]) {
		onChange(next);
	}

	function patch(id: string, partial: Partial<FieldCodecRule>) {
		update(rules.map((rule) => (rule.id === id ? { ...rule, ...partial } : rule)));
	}

	function patchAes(id: string, partial: Partial<FieldCodecRule['aes']>) {
		update(
			rules.map((rule) => (rule.id === id ? { ...rule, aes: { ...rule.aes, ...partial } } : rule))
		);
	}

	function addRule() {
		const column = columns.length === 1 ? columns[0] : '';
		update([...rules, newFieldCodecRule(uid('codec'), column)]);
	}

	function issueText(issue: CodecRuleProblem): string {
		const key = `codec.issue.${issue.issue}` as MessageKey;
		return t(key, { column: issue.column });
	}
</script>

<div class="codec-panel">
	<p class="codec-hint" title={t('codec.hint')}>{t('codec.hint')}</p>
	{#each rules as rule (rule.id)}
		{@const issue = issueById.get(rule.id)}
		<div class="codec-rule" class:is-on={rule.enabled}>
			<label class="codec-enable">
				<input
					type="checkbox"
					checked={rule.enabled}
					onchange={(event) => patch(rule.id, { enabled: event.currentTarget.checked })}
				/>
				{t('codec.enable')}
			</label>
			<div class="codec-line">
				<input
					class="codec-column"
					list={listId}
					value={rule.column}
					placeholder={t('codec.column')}
					aria-label={t('codec.column')}
					disabled={rule.enabled}
					oninput={(event) => patch(rule.id, { column: event.currentTarget.value })}
				/>
				<select
					aria-label={t('codec.op.base64')}
					value={rule.op}
					disabled={rule.enabled}
					onchange={(event) => patch(rule.id, { op: event.currentTarget.value as FieldCodecOp })}
				>
					<option value="base64">{t('codec.op.base64')}</option>
					<option value="hex">{t('codec.op.hex')}</option>
					<option value="uri">{t('codec.op.uri')}</option>
					<option value="aes">{t('codec.op.aes')}</option>
				</select>
				<button
					class="btn"
					type="button"
					onclick={() => update(rules.filter((item) => item.id !== rule.id))}
				>
					{t('codec.remove')}
				</button>
			</div>
			{#if rule.op === 'aes'}
				<div class="codec-line">
					<select
						aria-label={t('codec.input')}
						value={rule.aes.input}
						disabled={rule.enabled}
						onchange={(event) =>
							patchAes(rule.id, { input: event.currentTarget.value as CipherEncoding })}
					>
						<option value="base64">{t('codec.input.base64')}</option>
						<option value="hex">{t('codec.input.hex')}</option>
					</select>
					<select
						aria-label={t('codec.mode')}
						value={rule.aes.mode}
						disabled={rule.enabled}
						onchange={(event) => patchAes(rule.id, { mode: event.currentTarget.value as AesMode })}
					>
						<option value="ECB">{t('codec.mode.ECB')}</option>
						<option value="CBC">{t('codec.mode.CBC')}</option>
						<option value="GCM">{t('codec.mode.GCM')}</option>
					</select>
				</div>
				<div class="codec-line">
					<input
						type={revealed[rule.id] ? 'text' : 'password'}
						value={rule.aes.key}
						placeholder={t('codec.key')}
						aria-label={t('codec.key')}
						autocomplete="off"
						spellcheck="false"
						disabled={rule.enabled}
						oninput={(event) => patchAes(rule.id, { key: event.currentTarget.value })}
					/>
					<select
						aria-label={t('codec.keyEncoding')}
						value={rule.aes.keyEncoding}
						disabled={rule.enabled}
						onchange={(event) =>
							patchAes(rule.id, { keyEncoding: event.currentTarget.value as ByteEncoding })}
					>
						<option value="utf8">{t('codec.enc.utf8')}</option>
						<option value="hex">{t('codec.enc.hex')}</option>
						<option value="base64">{t('codec.enc.base64')}</option>
					</select>
					<button
						class="btn"
						type="button"
						disabled={rule.enabled}
						onclick={() => (revealed = { ...revealed, [rule.id]: !revealed[rule.id] })}
					>
						{revealed[rule.id] ? t('codec.hideKey') : t('codec.showKey')}
					</button>
				</div>
				{#if rule.aes.mode !== 'ECB'}
					<div class="codec-line">
						<input
							value={rule.aes.iv}
							placeholder={t('codec.iv')}
							aria-label={t('codec.iv')}
							disabled={rule.enabled || rule.aes.ivInCipher}
							spellcheck="false"
							oninput={(event) => patchAes(rule.id, { iv: event.currentTarget.value })}
						/>
						<select
							aria-label={t('codec.ivEncoding')}
							value={rule.aes.ivEncoding}
							disabled={rule.enabled || rule.aes.ivInCipher}
							onchange={(event) =>
								patchAes(rule.id, { ivEncoding: event.currentTarget.value as ByteEncoding })}
						>
							<option value="utf8">{t('codec.enc.utf8')}</option>
							<option value="hex">{t('codec.enc.hex')}</option>
							<option value="base64">{t('codec.enc.base64')}</option>
						</select>
						<label class="codec-check">
							<input
								type="checkbox"
								checked={rule.aes.ivInCipher}
								disabled={rule.enabled}
								onchange={(event) => patchAes(rule.id, { ivInCipher: event.currentTarget.checked })}
							/>
							{t('codec.ivInCipher')}
						</label>
					</div>
					{#if rule.aes.mode === 'CBC' && !rule.aes.iv && !rule.aes.ivInCipher}
						<p class="codec-note">{t('codec.ivEmpty')}</p>
					{/if}
				{/if}
			{/if}
			{#if issue}
				<p class="codec-issue" class:codec-hint-issue={issue.issue === 'missing-key'}>
					{issueText(issue)}
				</p>
			{/if}
		</div>
	{/each}
	<datalist id={listId}>
		{#each columns as name, index (`${index}:${name}`)}
			<option value={name}></option>
		{/each}
	</datalist>
	<button class="btn" type="button" onclick={addRule}>{t('codec.add')}</button>
</div>

<style>
	.codec-panel {
		flex: none;
		box-sizing: border-box;
		width: 100%;
		max-width: 100%;
		min-width: 0;
		height: 100%;
		padding: 8px 10px;
		overflow: auto;
		border-bottom: 1px solid var(--border);
		background: var(--bg-muted);
	}

	.codec-panel p {
		margin: 0 0 8px;
		max-width: 100%;
		color: var(--text-muted);
		font-size: 12px;
		overflow-wrap: anywhere;
	}

	.codec-hint {
		display: -webkit-box;
		overflow: hidden;
		-webkit-box-orient: vertical;
		line-clamp: 2;
		-webkit-line-clamp: 2;
	}

	.codec-rule {
		min-width: 0;
		margin-bottom: 8px;
		padding: 8px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: white;
	}

	.codec-rule.is-on {
		background: var(--bg-muted);
	}

	.codec-enable {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 8px;
		color: var(--text);
		font-size: 12px;
		font-weight: 600;
	}

	.codec-rule.is-on .codec-enable {
		color: var(--accent);
	}

	.codec-rule.is-on .codec-line input:disabled,
	.codec-rule.is-on .codec-line select:disabled {
		border-color: transparent;
		background: transparent;
		color: var(--text);
		opacity: 1;
		cursor: default;
	}

	.codec-line {
		display: grid;
		grid-template-columns: minmax(8rem, 20rem) minmax(0, 11rem) auto;
		justify-content: start;
		gap: 8px;
		align-items: center;
		min-width: 0;
	}

	.codec-column {
		width: 20rem;
		max-width: 100%;
	}

	.codec-line + .codec-line {
		margin-top: 8px;
	}

	.codec-line input,
	.codec-line select,
	.codec-line .btn {
		width: 100%;
		max-width: 100%;
		min-width: 0;
	}

	.codec-line .btn {
		width: auto;
		white-space: nowrap;
	}

	.codec-line input,
	.codec-line select {
		height: 30px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: white;
		padding: 0 8px;
	}

	.codec-check {
		display: inline-flex;
		grid-column: 1 / -1;
		align-items: center;
		gap: 6px;
		min-width: 0;
		color: var(--text);
		font-size: 12px;
	}

	.codec-note,
	.codec-issue {
		margin: 6px 0 0;
	}

	.codec-issue {
		color: var(--danger);
	}

	.codec-hint-issue {
		color: var(--text-muted);
	}
</style>
