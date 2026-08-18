<script lang="ts">
	let { src }: { src: string } = $props();
</script>

<span class="icon" style:--icon="url('{src}')" aria-hidden="true"></span>

<style>
	.icon {
		display: inline-block;
		flex-shrink: 0;
		width: 14px;
		height: 14px;
		background-color: var(--muted);
		-webkit-mask: var(--icon) center / contain no-repeat;
		mask: var(--icon) center / contain no-repeat;
		-webkit-mask-source-type: alpha;
		mask-mode: alpha;
	}
</style>
