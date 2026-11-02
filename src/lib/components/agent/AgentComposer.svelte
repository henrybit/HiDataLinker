<script lang="ts">
	import { ArrowUp, Paperclip } from '@lucide/svelte';
	import { t } from '$lib/i18n/i18n.svelte';

	let {
		value = $bindable(''),
		placeholder,
		hint,
		disabled = false,
		onSubmit
	}: {
		value?: string;
		placeholder?: string;
		hint?: string;
		disabled?: boolean;
		onSubmit?: () => void;
	} = $props();

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			if (!disabled && value.trim()) onSubmit?.();
		}
	}
</script>

<div class="agent-composer">
	<div class="agent-composer-bar">
		<button type="button" class="agent-composer-icon" title={t('agentMode.composer.attach')} disabled>
			<Paperclip size={16} />
		</button>
		<input
			class="agent-composer-input"
			type="text"
			placeholder={placeholder ?? t('agentMode.composer.placeholder')}
			bind:value
			{disabled}
			onkeydown={onKeydown}
		/>
		<span class="agent-composer-badge">{t('agentMode.composer.badge')}</span>
		<button
			type="button"
			class="agent-composer-send"
			disabled={disabled || !value.trim()}
			onclick={() => onSubmit?.()}
			aria-label={t('agentMode.composer.send')}
		>
			<ArrowUp size={16} />
		</button>
	</div>
	<p class="agent-composer-hint">{hint ?? t('agentMode.composer.hint')}</p>
</div>
