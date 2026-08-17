<script lang="ts">
	import type { HistoryItem } from '$lib/types';
	import Button from './Button.svelte';
	import ReceiptCard from './ReceiptCard.svelte';

	let {
		entries,
		selected,
		onselect,
		onback,
		onreveal
	}: {
		entries: HistoryItem[];
		selected: number | null;
		onselect: (index: number) => void;
		onback: () => void;
		onreveal: (taskId: string, name: string) => void;
	} = $props();

	const detail = $derived(selected !== null ? entries[selected] : null);
</script>

{#if detail}
	<div class="flex w-full flex-col gap-2 pt-1">
		<Button label="Back" onclick={onback} />
		<div class="text-sm text-[var(--muted)]">{detail.status_mark} {detail.title}</div>
		{#if detail.receipt}
			<ReceiptCard
				receipt={detail.receipt}
				names={detail.artifact_names}
				onreveal={(name) => onreveal(detail.id, name)}
			/>
		{:else}
			<div class="text-sm text-[var(--muted)]">
				{detail.status === 'failed'
					? 'This task did not finish.'
					: detail.status === 'cancelled'
						? 'This task was cancelled.'
						: detail.status === 'running'
							? 'This task was interrupted.'
							: 'No receipt saved.'}
			</div>
		{/if}
	</div>
{:else if entries.length === 0}
	<div class="pt-2 text-sm text-[var(--muted)]">No recent tasks</div>
{:else}
	<div class="flex w-full flex-col">
		{#each entries as entry, index (entry.id)}
			{#if index === 0 || entry.group !== entries[index - 1].group}
				<div class="pt-2 text-xs text-[var(--muted)]">{entry.group}</div>
			{/if}
			<button
				type="button"
				class="cursor-pointer py-1 text-left text-sm hover:opacity-80"
				onclick={() => onselect(index)}
			>
				{entry.status_mark} {entry.title}
			</button>
		{/each}
	</div>
{/if}
