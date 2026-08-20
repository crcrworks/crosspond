<script lang="ts">
	import Badge from './Badge.svelte';
	import Button from './Button.svelte';

	let {
		title,
		description,
		body = 'prose',
		onallow,
		oncancel
	}: {
		title: string;
		description: string;
		body?: 'prose' | 'command';
		onallow: () => void;
		oncancel: () => void;
	} = $props();
</script>

<div class="surface mt-2 flex flex-col gap-2">
	<Badge label="Needs allow" tone="yellow" />
	<div class="text-sm">Crosspond wants to:</div>
	<div class="text-sm">{title}</div>
	{#if description}
		{#if body === 'command'}
			<pre class="command-body">{description}</pre>
		{:else}
			<div class="text-sm text-[var(--muted)]">{description}</div>
		{/if}
	{/if}
	<div class="flex flex-row gap-2">
		<Button label="Allow" onclick={onallow} variant="primary" />
		<Button label="Cancel" onclick={oncancel} />
	</div>
</div>

<style>
	.command-body {
		max-height: 12rem;
		overflow: auto;
		margin: 0;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--kbd-bg);
		padding: 8px 10px;
		font-family: 'SF Mono', ui-monospace, Menlo, monospace;
		font-size: 0.75rem;
		line-height: 1.45;
		color: var(--text);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		user-select: text;
	}
</style>
