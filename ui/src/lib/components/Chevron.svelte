<script lang="ts">
	let { expanded = false }: { expanded?: boolean } = $props();
</script>

<svg
	class={['chevron group-hover:opacity-80', expanded && 'open']}
	viewBox="0 0 16 16"
	width="16"
	height="16"
	aria-hidden="true"
>
	<path
		d="M4 6.25 8 10.25 12 6.25"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	/>
</svg>

<style>
	.chevron {
		display: block;
		flex-shrink: 0;
		color: var(--muted);
		transform-origin: center;
		transform: rotate(-90deg);
	}

	.chevron.open {
		transform: none;
	}
</style>
