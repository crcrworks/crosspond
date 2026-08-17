<script lang="ts">
	import Button from './Button.svelte';

	let {
		ready,
		hint,
		onsettings,
		oncontinue,
		ondone
	}: {
		ready: boolean;
		hint: string | null;
		onsettings: () => void;
		oncontinue: () => void;
		ondone: () => void;
	} = $props();
</script>

{#if ready}
	<div class="flex flex-col gap-3 pt-2">
		<div class="text-sm">Crosspond is ready.</div>
		<div class="text-sm text-[var(--muted)]">Press Option + Space anywhere.</div>
		<Button label="Done" onclick={ondone} />
	</div>
{:else}
	<div class="flex flex-col gap-3 pt-2">
		<div class="text-sm">Bring your own AI.</div>
		<div class="text-sm text-[var(--muted)]">
			Set a provider, model, and API key in Settings. Accessibility is not required for chat.
		</div>
		{#if hint}
			<div class="text-sm text-[var(--danger)]">{hint}</div>
		{/if}
		<div class="flex flex-row gap-2">
			<Button label="Open Settings" onclick={onsettings} />
			<Button label="Continue" onclick={oncontinue} />
		</div>
	</div>
{/if}
