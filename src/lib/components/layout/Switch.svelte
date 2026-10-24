<script lang="ts">
	let {
		checked = false,
		disabled = false,
		name,
		onText,
		offText,
		onToggle
	}: {
		checked?: boolean;
		disabled?: boolean;
		name: string;
		onText: string;
		offText: string;
		onToggle?: (enabled: boolean) => void;
	} = $props();

	function change(event: Event) {
		onToggle?.((event.currentTarget as HTMLInputElement).checked);
	}
</script>

<label class="apple-switch" class:is-disabled={disabled}>
	<input
		type="checkbox"
		role="switch"
		aria-label={name}
		{checked}
		{disabled}
		onchange={change}
	/>
	<span class="apple-switch-ui" aria-hidden="true">
		<span class="apple-switch-knob"></span>
	</span>
	<span class="apple-switch-text">
		<span class="when-on">{onText}</span>
		<span class="when-off">{offText}</span>
	</span>
</label>

<style>
	.apple-switch {
		display: inline-flex;
		flex: none;
		align-items: center;
		gap: 8px;
		color: #8e8e93;
		cursor: pointer;
		white-space: nowrap;
	}

	.apple-switch.is-disabled {
		cursor: default;
		opacity: 0.55;
	}

	.apple-switch input {
		position: absolute;
		width: 1px;
		height: 1px;
		margin: 0;
		opacity: 0;
	}

	.apple-switch-ui {
		position: relative;
		width: 44px;
		height: 26px;
		border-radius: 999px;
		background: #e9e9eb;
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.06);
		transition: background 0.2s ease;
	}

	.apple-switch-knob {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 22px;
		height: 22px;
		border-radius: 50%;
		background: #fff;
		box-shadow:
			0 2px 5px rgba(0, 0, 0, 0.22),
			0 0 0 0.5px rgba(0, 0, 0, 0.05);
		transition:
			transform 0.22s cubic-bezier(0.2, 0.85, 0.32, 1.2),
			width 0.22s cubic-bezier(0.2, 0.85, 0.32, 1.2);
	}

	.apple-switch:has(input:checked) {
		color: #1f8a3b;
		font-weight: 600;
	}

	.apple-switch:has(input:checked) .apple-switch-ui {
		background: #34c759;
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.04);
	}

	.apple-switch:has(input:checked) .apple-switch-knob {
		transform: translateX(18px);
	}

	.apple-switch:active:not(.is-disabled) .apple-switch-knob {
		width: 26px;
	}

	.apple-switch:has(input:checked):active:not(.is-disabled) .apple-switch-knob {
		transform: translateX(14px);
	}

	.apple-switch input:focus-visible + .apple-switch-ui {
		outline: 2px solid #34c759;
		outline-offset: 2px;
	}

	.when-on {
		display: none;
	}

	.apple-switch:has(input:checked) .when-on {
		display: inline;
	}

	.apple-switch:has(input:checked) .when-off {
		display: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.apple-switch-ui,
		.apple-switch-knob {
			transition: none;
		}
	}
</style>
