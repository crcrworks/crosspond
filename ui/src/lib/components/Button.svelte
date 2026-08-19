<script lang="ts">
	let {
		label,
		onclick,
		variant = 'ghost',
		type = 'button',
		disabled = false
	}: {
		label: string;
		onclick?: () => void;
		variant?: 'ghost' | 'primary' | 'danger';
		type?: 'button' | 'submit';
		disabled?: boolean;
	} = $props();
</script>

<button {type} class={['btn', variant]} {onclick} {disabled} data-tauri-drag-region="false">
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

	.primary:hover:not(:disabled) {
		background: var(--button-primary-hover);
		border-color: var(--button-primary-hover);
	}

	.btn:disabled {
		opacity: 0.35;
		cursor: default;
	}

	.btn:disabled:active {
		transform: none;
	}

	.danger {
		background: var(--tone-red-bg);
		border-color: var(--tone-red);
		color: var(--tone-red);
	}
</style>
