<script lang="ts">
	let { label, onclick, danger = false }: { label: string; onclick: () => void; danger?: boolean } =
		$props();
</script>

<button
	type="button"
	class="shrink-0 cursor-pointer rounded-sm border px-3 py-1 text-sm text-[var(--text)] active:opacity-85"
	style:background="var(--button)"
	style:border-color={danger ? 'var(--danger)' : 'var(--button-border)'}
	{onclick}
>
	{label}
</button>
