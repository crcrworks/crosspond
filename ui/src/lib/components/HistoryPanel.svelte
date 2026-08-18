<script lang="ts">
	import { taskStatusVisual } from '$lib/tools';
	import type { HistoryItem } from '$lib/types';
	import Badge from './Badge.svelte';
	import Button from './Button.svelte';
	import ReceiptCard from './ReceiptCard.svelte';

	let {
		entries,
		selected,
		onselect,
		onback,
		onreveal,
		showBack = true
	}: {
		entries: HistoryItem[];
		selected: number | null;
		onselect: (index: number) => void;
		onback: () => void;
		onreveal: (taskId: string, name: string) => void;
		showBack?: boolean;
	} = $props();

	const detail = $derived(selected !== null ? entries[selected] : null);
</script>

{#if detail}
	{@const status = taskStatusVisual(detail.status)}
	<div class="flex w-full flex-col gap-2 pt-1">
		{#if showBack}
			<Button label="Back" onclick={onback} />
		{/if}
		<div class="flex flex-row items-center gap-2">
			<Badge label={status.label} tone={status.tone} />
			<div class="min-w-0 truncate text-sm">{detail.title}</div>
		</div>
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
			{@const status = taskStatusVisual(entry.status)}
			{#if index === 0 || entry.group !== entries[index - 1].group}
				<div class="pt-2 text-xs text-[var(--muted)]">{entry.group}</div>
			{/if}
			<button
				type="button"
				class="flex cursor-pointer flex-row items-center gap-2 py-1 text-left hover:opacity-80"
				onclick={() => onselect(index)}
			>
				<Badge label={status.label} tone={status.tone} />
				<div class="min-w-0 flex-1 truncate text-sm">{entry.title}</div>
			</button>
		{/each}
	</div>
{/if}
