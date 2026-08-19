<script lang="ts">
	import type { HistoryItem } from '$lib/types';
	import Icon from './Icon.svelte';

	let {
		liveTitle,
		liveActive,
		entries,
		selectedId,
		onnew,
		onlive,
		onselect
	}: {
		liveTitle: string | null;
		liveActive: boolean;
		entries: HistoryItem[];
		selectedId: string | null;
		onnew: () => void;
		onlive: () => void;
		onselect: (id: string) => void;
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
</div>
