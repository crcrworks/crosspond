<script lang="ts">
	import type { Attachment } from 'svelte/attachments';
	import Badge from './Badge.svelte';
	import Button from './Button.svelte';

	let {
		title,
		credentialRef,
		saveOffered,
		onfill,
		oncancel,
		oncompositionstart = () => {},
		oncompositionend = () => {}
	}: {
		title: string;
		credentialRef: string;
		saveOffered: boolean;
		onfill: (username: string, password: string, save: boolean) => void;
		oncancel: () => void;
		oncompositionstart?: () => void;
		oncompositionend?: () => void;
	} = $props();

	let username = $state('');
	let password = $state('');
	let save = $state(false);

	const canFill = $derived(username.trim().length > 0 && password.trim().length > 0);

	const focusUsername: Attachment<HTMLInputElement> = (node) => {
		node.focus();
	};

	function clearFields() {
		username = '';
		password = '';
		save = false;
	}

	function handleSubmit(event: Event) {
		event.preventDefault();
		if (!canFill) return;
		onfill(username, password, saveOffered && save);
		clearFields();
	}

	function handleCancel() {
		clearFields();
		oncancel();
	}
</script>

<div class="surface mt-2 flex flex-col gap-2">
	<form class="flex flex-col gap-2" onsubmit={handleSubmit}>
		<Badge label="Needs login" tone="yellow" />
		<div class="text-sm">{title}</div>
		<div class="text-sm text-[var(--muted)]">{credentialRef}</div>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Username</span>
			<input
				{@attach focusUsername}
				bind:value={username}
				type="text"
				autocomplete="off"
				spellcheck={false}
				autocapitalize="off"
				class="rounded-md border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
				oncompositionstart={oncompositionstart}
				oncompositionend={oncompositionend}
			/>
		</label>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Password</span>
			<input
				bind:value={password}
				type="password"
				class="rounded-md border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
				oncompositionstart={oncompositionstart}
				oncompositionend={oncompositionend}
			/>
		</label>
		{#if saveOffered}
			<div class="flex flex-row items-center gap-2">
				<span class="text-sm">Save in Keychain</span>
				<button
					type="button"
					class="save-switch"
					role="switch"
					aria-checked={save}
					aria-label="Save in Keychain"
					onclick={() => (save = !save)}
				>
					<span class={['save-switch-track', save && 'on']} aria-hidden="true">
						<span class="save-switch-knob"></span>
					</span>
					<span>{save ? 'On' : 'Off'}</span>
				</button>
			</div>
		{/if}
		<div class="flex flex-row gap-2">
			<Button label="Fill" variant="primary" type="submit" disabled={!canFill} />
			<Button label="Cancel" onclick={handleCancel} />
		</div>
	</form>
</div>

<style>
	.save-switch {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--tone-muted-bg);
		padding: 2px 8px;
		font: inherit;
		font-size: 11px;
		line-height: 1.4;
		color: var(--text);
		cursor: pointer;
	}

	.save-switch:hover {
		background: var(--surface);
	}

	.save-switch-track {
		position: relative;
		width: 22px;
		height: 12px;
		flex-shrink: 0;
		border-radius: 6px;
		background: var(--border);
	}

	.save-switch-track.on {
		background: var(--text);
	}

	.save-switch-knob {
		position: absolute;
		top: 1px;
		left: 1px;
		width: 10px;
		height: 10px;
		border-radius: 6px;
		background: var(--surface);
	}

	.save-switch-track.on .save-switch-knob {
		transform: translateX(10px);
	}
</style>
