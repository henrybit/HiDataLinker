<script lang="ts">
	import { marked } from 'marked';

	let { content = '' }: { content?: string } = $props();

	const html = $derived.by(() => {
		const raw = content?.trim() ? content : '';
		marked.setOptions({ gfm: true, breaks: true });
		return marked.parse(raw, { async: false }) as string;
	});
</script>

<div class="agent-md">{@html html}</div>
