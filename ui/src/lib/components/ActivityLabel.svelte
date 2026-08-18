<script lang="ts">
	let { text, running = false }: { text: string; running?: boolean } = $props();
	const line = $derived(text.replace(/[\n\r]/g, ' '));
</script>

<span class={['label', running && 'shimmer']}>{line}</span>

<style>
	.label {
		display: inline-block;
		max-width: 100%;
		overflow: hidden;
		text-align: left;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--muted);
	}

	.shimmer {
		background-color: var(--muted);
		background-image: linear-gradient(
			90deg,
			var(--muted) 0%,
			var(--muted) 28%,
			#fff 38%,
			#fff 62%,
			var(--muted) 72%,
			var(--muted) 100%
		);
		background-size: 200% 100%;
		-webkit-background-clip: text;
		background-clip: text;
		-webkit-text-fill-color: transparent;
		color: transparent;
		animation: sheen 1.2s linear infinite;
	}

	@media (prefers-color-scheme: light) {
		.shimmer {
			background-image: linear-gradient(
				90deg,
				var(--muted) 0%,
				var(--muted) 28%,
				#000 38%,
				#000 62%,
				var(--muted) 72%,
				var(--muted) 100%
			);
		}
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
