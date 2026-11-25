<script lang="ts">
	import { onMount } from 'svelte';
	import { getVersion } from '@tauri-apps/api/app';
	import { version as bundledVersion } from '../../../../package.json';
	import { isTauriRuntime } from '$lib/api/tauri';
	import { aboutDialog } from '$lib/about.svelte';
	import { t } from '$lib/i18n/i18n.svelte';

	let version = $state(bundledVersion);

	function close() {
		aboutDialog.open = false;
	}

	onMount(() => {
		const onKey = (event: KeyboardEvent) => {
			if (event.key === 'Escape') close();
		};
		window.addEventListener('keydown', onKey);
		if (isTauriRuntime()) {
			void getVersion()
				.then((value) => {
					if (value.trim()) version = value.trim();
				})
				.catch(() => {});
		}
		return () => window.removeEventListener('keydown', onKey);
	});
</script>

<div class="modal-backdrop">
	<div class="modal about" role="dialog" aria-modal="true" aria-labelledby="about-title">
		<header id="about-title">{t('dialog.aboutTitle')}</header>
		<div class="body">
			<p class="about-name">{t('app.title')}</p>
			<p class="about-version">{t('dialog.aboutVersion', { version })}</p>
		</div>
		<footer>
			<button class="btn" type="button" onclick={close}>{t('dialog.close')}</button>
		</footer>
	</div>
</div>
