<script lang="ts">
	import type { Receipt } from '$lib/types';
	import Button from './Button.svelte';
	import ToolChip from './ToolChip.svelte';

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

{#if artifacts.length > 0}
	<div class="pt-2">
		<div class="surface flex flex-col gap-2">
			<div class="text-xs uppercase tracking-[0.05em] text-[var(--muted)]">Artifacts</div>
			{#each artifacts as name (name)}
				<div class="flex flex-row items-center gap-2">
					<ToolChip src="/icons/file.svg" tone="yellow" />
					<div class="min-w-0 flex-1 text-sm">{name}</div>
					<Button label="Show in Finder" onclick={() => onreveal(name)} />
				</div>
			{/each}
		</div>
	</div>
{/if}
