<script lang="ts">
	import { Check, Copy } from '@lucide/svelte';
	import { formatDurationMs, formatMessageTime } from '$lib/agent/message-meta';
	import { t } from '$lib/i18n/i18n.svelte';

	let {
		at,
		durationMs = null,
		text,
		align = 'start'
	}: {
		at: string;
		durationMs?: number | null;
		text: string;
		align?: 'start' | 'end';
	} = $props();

	let copied = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | null = null;

	const timeLabel = $derived(formatMessageTime(at));
	const durationLabel = $derived(
		durationMs != null ? formatDurationMs(durationMs) : ''
	);
	const canCopy = $derived(text.trim().length > 0);

	async function copy() {
		if (!canCopy) return;
		try {
			await navigator.clipboard.writeText(text);
			copied = true;
			if (copyTimer) clearTimeout(copyTimer);
			copyTimer = setTimeout(() => {
				copied = false;
				copyTimer = null;
			}, 1500);
		} catch {
			copied = false;
		}
	}
</script>

<div class="agent-msg-meta" class:end={align === 'end'}>
	{#if timeLabel}
		<span class="agent-msg-meta-time" title={at}>{timeLabel}</span>
	{/if}
	{#if durationLabel}
		<span class="agent-msg-meta-sep" aria-hidden="true">·</span>
		<span class="agent-msg-meta-duration"
			>{t('agentMode.live.totalDuration', { duration: durationLabel })}</span
		>
	{/if}
	<button
		type="button"
		class="agent-msg-copy"
		class:copied
		disabled={!canCopy}
		title={copied ? t('agentMode.live.copied') : t('agentMode.live.copy')}
		aria-label={copied ? t('agentMode.live.copied') : t('agentMode.live.copy')}
		onclick={copy}
	>
		{#if copied}
			<Check size={13} />
		{:else}
			<Copy size={13} />
		{/if}
	</button>
</div>
