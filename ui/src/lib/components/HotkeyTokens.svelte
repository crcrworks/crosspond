<script lang="ts">
	let { tokens }: { tokens: string[] } = $props();
</script>

{#each tokens as token, i (token + String(i))}
	{#if i > 0}<span class="sep">+</span>{/if}<kbd>{token}</kbd>
{/each}

<style>
	.sep {
		margin: 0 4px;
		color: var(--muted);
	}
</style>
