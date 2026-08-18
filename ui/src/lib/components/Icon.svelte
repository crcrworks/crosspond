<script lang="ts">
	let { src }: { src: string } = $props();
</script>

<span class="icon" style:--icon="url('{src}')" aria-hidden="true"></span>

<style>
	.icon {
		display: block;
		flex-shrink: 0;
		width: 14px;
		height: 14px;
		overflow: hidden;
		background-color: var(--muted);
		-webkit-mask-image: var(--icon);
		-webkit-mask-size: contain;
		-webkit-mask-repeat: no-repeat;
		-webkit-mask-position: center;
		transform: translateZ(0);
	}
</style>
