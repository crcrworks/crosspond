<script lang="ts">
	import Button from './Button.svelte';
	import HotkeyTokens from './HotkeyTokens.svelte';

	let {
		ready,
		hint,
		hotkeyTokens,
		onsettings,
		oncontinue,
		ondone
	}: {
		ready: boolean;
		hint: string | null;
		hotkeyTokens: string[];
		onsettings: () => void;
		oncontinue: () => void;
		ondone: () => void;
	} = $props();
</script>

{#if ready}
	<div class="flex flex-col gap-3 pt-2" data-tauri-drag-region="false">
		<div class="text-sm">Crosspond is ready.</div>
		<div class="text-sm text-[var(--muted)]">
			Press <HotkeyTokens tokens={hotkeyTokens} /> anywhere to open the command bar.
		</div>
		<Button label="Open" onclick={ondone} variant="primary" />
	</div>
{:else}
	<div class="flex flex-col gap-3 pt-2" data-tauri-drag-region="false">
		<div class="text-sm">Bring your own AI.</div>
		<div class="text-sm text-[var(--muted)]">
			Set a provider, model, and API key in Settings. Accessibility is not required for chat.
		</div>
		{#if hint}
			<div class="text-sm text-[var(--danger)]">{hint}</div>
		{/if}
		<div class="flex flex-row gap-2">
			<Button label="Open Settings" onclick={onsettings} />
			<Button label="Continue" onclick={oncontinue} variant="primary" />
		</div>
	</div>
{/if}
