<script lang="ts">
	import Badge from './Badge.svelte';
	import Button from './Button.svelte';

	let {
		title,
		description,
		onallow,
		oncancel
	}: {
		title: string;
		description: string;
		onallow: () => void;
		oncancel: () => void;
	} = $props();
</script>

<div class="surface mt-2 flex flex-col gap-2">
	<Badge label="Needs allow" tone="yellow" />
	<div class="text-sm">Crosspond wants to:</div>
	<div class="text-sm">{title}</div>
	{#if description}
		<div class="text-sm text-[var(--muted)]">{description}</div>
	{/if}
	<div class="flex flex-row gap-2">
		<Button label="Allow" onclick={onallow} variant="primary" />
		<Button label="Cancel" onclick={oncancel} />
	</div>
</div>
