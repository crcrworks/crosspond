<script lang="ts">
	import type { Receipt } from '$lib/types';
	import Button from './Button.svelte';

	let {
		receipt,
		names,
		onreveal
	}: {
		receipt: Receipt;
		names?: string[];
		onreveal: (name: string) => void;
	} = $props();

	const artifacts = $derived(names ?? receipt.artifacts);
</script>

{#if receipt.actions.length > 0 || artifacts.length > 0}
	<div class="flex flex-col gap-1 pt-2">
		{#if receipt.actions.length > 0}
			<div class="pt-1 text-sm text-[var(--muted)]">Changed</div>
			{#each receipt.actions as line (line)}
				<div class="text-sm text-[var(--muted)]">• {line}</div>
			{/each}
		{/if}
		{#if artifacts.length > 0}
			<div class="pt-1 text-sm text-[var(--muted)]">Artifacts</div>
			{#each artifacts as name (name)}
				<div class="flex flex-row items-center gap-2">
					<div class="min-w-0 flex-1 text-sm">{name}</div>
					<Button label="Show in Finder" onclick={() => onreveal(name)} />
				</div>
			{/each}
		{/if}
	</div>
{/if}
