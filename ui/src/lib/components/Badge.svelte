<script lang="ts">
	import type { ToolTone } from '$lib/tools';

	let { label, tone = 'muted' }: { label: string; tone?: ToolTone } = $props();
</script>

<span class="pill" data-tone={tone}>{label}</span>
