<script lang="ts">
	import type { ToolTone } from '$lib/tools';
	import Icon from './Icon.svelte';

	let {
		src,
		tone = 'muted',
		running = false
	}: { src: string; tone?: ToolTone; running?: boolean } = $props();
</script>

<span class={['chip', running && 'running']} data-tone={tone} aria-hidden="true">
	<Icon {src} color="var(--tone-fg)" size={12} />
</span>

<style>
	.chip {
		display: inline-flex;
		flex-shrink: 0;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		border-radius: 6px;
		background: var(--tone-bg);
	}

	.running {
		animation: pulse 1.2s ease-in-out infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.45;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.running {
			animation: none;
		}
	}
</style>
