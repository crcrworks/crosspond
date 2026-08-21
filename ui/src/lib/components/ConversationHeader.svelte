<script lang="ts">
	import type { HistoryItem } from '$lib/types';
	import type { UpdateNotice } from '$lib/updater';
	import Icon from './Icon.svelte';

	let {
		liveTitle,
		liveActive,
		entries,
		selectedId,
		onnew,
		onlive,
		onselect,
		updateNotice = 'hidden',
		onupdate,
		ondismissupdate
	}: {
		liveTitle: string | null;
		liveActive: boolean;
		entries: HistoryItem[];
		selectedId: string | null;
		onnew: () => void;
		onlive: () => void;
		onselect: (id: string) => void;
		updateNotice?: UpdateNotice;
		onupdate?: () => void;
		ondismissupdate?: () => void;
	} = $props();
</script>

<div class="chat-header" data-tauri-drag-region>
	<button type="button" class="chat-new" aria-label="New chat" onclick={onnew}>
		<Icon src="/icons/plus.svg" />
	</button>
	<div class="chat-tabs" data-tauri-drag-region>
		{#if liveTitle !== null}
			<button type="button" class={['chat-tab', { active: liveActive }]} onclick={onlive}>
				{liveTitle}
			</button>
		{/if}
		{#each entries as entry (entry.id)}
			<button
				type="button"
				class={['chat-tab', { active: selectedId === entry.id }]}
				onclick={() => onselect(entry.id)}
			>
				{entry.title}
			</button>
		{/each}
	</div>
	{#if updateNotice !== 'hidden'}
		<div class="chat-update" data-tauri-drag-region="false">
			{#if updateNotice === 'installing'}
				<span class="chat-update-label">Updating…</span>
			{:else}
				<button type="button" class="chat-update-label" onclick={onupdate}>
					Update available
				</button>
				<button
					type="button"
					class="chat-update-dismiss"
					aria-label="Dismiss update"
					onclick={ondismissupdate}
				>
					×
				</button>
			{/if}
		</div>
	{/if}
</div>
