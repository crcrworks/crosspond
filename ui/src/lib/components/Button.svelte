<script lang="ts">
	let {
		label,
		onclick,
		variant = 'ghost'
	}: {
		label: string;
		onclick: () => void;
		variant?: 'ghost' | 'primary' | 'danger';
	} = $props();
</script>

<button type="button" class={['btn', variant]} {onclick}>
	{label}
</button>

<style>
	.btn {
		flex-shrink: 0;
		cursor: pointer;
		border: 1px solid var(--button-border);
		border-radius: 6px;
		background: var(--button);
		padding: 4px 12px;
		font-size: 0.875rem;
		line-height: 1.25rem;
		color: var(--text);
	}

	.btn:active {
		transform: scale(0.98);
	}

	.primary {
		background: var(--button-primary);
		border-color: var(--button-primary);
		color: var(--button-primary-text);
	}

	.primary:hover {
		background: var(--button-primary-hover);
		border-color: var(--button-primary-hover);
	}

	.danger {
		background: var(--tone-red-bg);
		border-color: var(--tone-red);
		color: var(--tone-red);
	}
</style>
