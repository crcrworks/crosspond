<script lang="ts">
	import { openExternalUrl } from '$lib/api';
	import { externalUrlFromClick } from '$lib/external-links';
	import '../app.css';

	let { children } = $props();

	function onLinkClick(event: MouseEvent) {
		const url = externalUrlFromClick(event, location.origin);
		if (!url) return;
		event.preventDefault();
		void openExternalUrl(url);
	}
</script>

<svelte:window onclick={onLinkClick} onauxclick={onLinkClick} />

{@render children()}
