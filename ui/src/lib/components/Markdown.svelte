<script lang="ts">
	import { renderMarkdown } from '$lib/markdown';

	let { source }: { source: string } = $props();
	const html = $derived(renderMarkdown(source.trimEnd()));
</script>

<div class="markdown text-sm leading-[1.65] break-words">{@html html}</div>

<style>
	.markdown :global(p) {
		margin: 0.55em 0;
	}
	.markdown :global(p:first-child) {
		margin-top: 0;
	}
	.markdown :global(p:last-child) {
		margin-bottom: 0;
	}
	.markdown :global(a) {
		color: var(--link);
	}
	.markdown :global(code) {
		background: var(--code-bg);
		color: var(--code-fg);
		border-radius: 4px;
		padding: 0 4px;
	}
	.markdown :global(pre) {
		background: var(--code-bg);
		color: var(--code-fg);
		border-radius: 6px;
		padding: 8px;
		overflow-x: auto;
	}
	.markdown :global(blockquote) {
		border-left: 3px solid var(--quote);
		padding-left: 8px;
		color: var(--muted);
	}
	.markdown :global(table) {
		border-collapse: collapse;
		width: 100%;
	}
	.markdown :global(th),
	.markdown :global(td) {
		border: 1px solid var(--border);
		padding: 4px 8px;
		text-align: left;
	}
	.markdown :global(th) {
		background: var(--code-bg);
	}
</style>
