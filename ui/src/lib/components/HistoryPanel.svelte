<script lang="ts">
	import { taskStatusVisual } from '$lib/tools';
	import type { HistoryItem } from '$lib/types';
	import Badge from './Badge.svelte';

	let {
		entries,
		onselect
	}: {
		entries: HistoryItem[];
		onselect: (id: string) => void;
	} = $props();
</script>

{#if entries.length === 0}
	<div class="pt-2 text-sm text-[var(--muted)]">No recent chats</div>
{:else}
	<div class="flex w-full flex-col">
		{#each entries as entry, index (entry.id)}
			{@const status = taskStatusVisual(entry.status)}
			{#if index === 0 || entry.group !== entries[index - 1].group}
				<div class="pt-2 text-xs text-[var(--muted)]">{entry.group}</div>
			{/if}
			<button
				type="button"
				class="flex cursor-pointer flex-row items-center gap-2 py-1 text-left hover:opacity-80"
				onclick={() => onselect(entry.id)}
			>
				<Badge label={status.label} tone={status.tone} />
				<div class="min-w-0 flex-1 truncate text-sm">{entry.title}</div>
			</button>
		{/each}
	</div>
{/if}
