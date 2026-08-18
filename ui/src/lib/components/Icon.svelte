<script lang="ts">
	let {
		src,
		color = 'var(--muted)',
		size = 14
	}: { src: string; color?: string; size?: number } = $props();
</script>

<span
	class="icon"
	style:--icon="url('{src}')"
	style:--icon-color={color}
	style:width="{size}px"
	style:height="{size}px"
	aria-hidden="true"
></span>

<style>
	.icon {
		display: block;
		flex-shrink: 0;
		overflow: hidden;
		background-color: var(--icon-color, var(--muted));
		-webkit-mask-image: var(--icon);
		mask-image: var(--icon);
		-webkit-mask-size: contain;
		mask-size: contain;
		-webkit-mask-repeat: no-repeat;
		mask-repeat: no-repeat;
		-webkit-mask-position: center;
		mask-position: center;
		transform: translateZ(0);
	}
</style>
