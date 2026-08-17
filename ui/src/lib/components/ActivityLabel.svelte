<script lang="ts">
	let { text, running = false }: { text: string; running?: boolean } = $props();
	const line = $derived(text.replace(/[\n\r]/g, ' '));
</script>

<span class={['block overflow-hidden text-ellipsis whitespace-nowrap', running && 'shimmer']}>
	{line}
</span>

<style>
	.shimmer {
		background: linear-gradient(
			90deg,
			var(--muted) 0%,
			var(--text) 45%,
			var(--muted) 100%
		);
		background-size: 200% 100%;
		-webkit-background-clip: text;
		background-clip: text;
		color: transparent;
		animation: sheen 1.2s linear infinite;
	}

	@keyframes sheen {
		from {
			background-position: 100% 0;
		}
		to {
			background-position: -100% 0;
		}
	}
</style>
