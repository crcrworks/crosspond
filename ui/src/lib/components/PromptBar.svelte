<script lang="ts">
	import { APPROVAL_MODES, approvalLabel } from '$lib/tools';
	import { composerExtraHeight } from '$lib/launcher-size';
	import type { ComputerApproval } from '$lib/types';
	import Chevron from './Chevron.svelte';
	import Icon from './Icon.svelte';

	let {
		variant = 'seamless',
		value = $bindable(''),
		textarea = $bindable<HTMLTextAreaElement | undefined>(undefined),
		menuOpen = $bindable(false),
		placeholder,
		approval,
		busy = false,
		canSubmit = true,
		onsubmit,
		oncancel,
		onapproval,
		ongrow,
		oncompositionstart,
		oncompositionend
	}: {
		variant?: 'seamless' | 'docked';
		value: string;
		textarea?: HTMLTextAreaElement;
		menuOpen: boolean;
		placeholder: string;
		approval: ComputerApproval;
		busy?: boolean;
		canSubmit?: boolean;
		onsubmit: () => void;
		oncancel: () => void;
		onapproval: (mode: ComputerApproval) => void;
		ongrow: (extra: number) => void;
		oncompositionstart: () => void;
		oncompositionend: () => void;
	} = $props();

	let root: HTMLDivElement | undefined = $state();
	let activeIndex = $state(0);
	const docked = $derived(variant === 'docked');
	const sendReady = $derived(canSubmit && value.trim().length > 0);

	function captureRoot(node: HTMLDivElement) {
		root = node;
		return () => {
			if (root === node) root = undefined;
		};
	}

	function captureTextarea(node: HTMLTextAreaElement) {
		textarea = node;
		return () => {
			if (textarea === node) textarea = undefined;
		};
	}

	function resize() {
		if (!textarea) return;
		textarea.style.height = 'auto';
		ongrow(composerExtraHeight(textarea.scrollHeight));
		textarea.style.height = `${Math.min(textarea.scrollHeight, 160)}px`;
	}

	function onPromptKey(event: KeyboardEvent) {
		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			onsubmit();
		}
	}

	function openMenu() {
		activeIndex = Math.max(0, APPROVAL_MODES.indexOf(approval));
		menuOpen = true;
	}

	function closeMenu() {
		menuOpen = false;
	}

	function toggleMenu(event: MouseEvent) {
		event.preventDefault();
		event.stopPropagation();
		if (menuOpen) closeMenu();
		else openMenu();
	}

	function selectMode(mode: ComputerApproval) {
		closeMenu();
		if (mode !== approval) onapproval(mode);
	}

	function onWindowPointerDown(event: PointerEvent) {
		if (!menuOpen || !root) return;
		if (root.contains(event.target as Node)) return;
		closeMenu();
	}

	function onModeKey(event: KeyboardEvent) {
		if (event.key === 'Escape' && menuOpen) {
			event.preventDefault();
			closeMenu();
			return;
		}
		if (!menuOpen && (event.key === 'ArrowDown' || event.key === 'Enter' || event.key === ' ')) {
			event.preventDefault();
			openMenu();
			return;
		}
		if (!menuOpen) return;
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			activeIndex = (activeIndex + 1) % APPROVAL_MODES.length;
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			activeIndex = (activeIndex - 1 + APPROVAL_MODES.length) % APPROVAL_MODES.length;
		} else if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			const mode = APPROVAL_MODES[activeIndex];
			if (mode) selectMode(mode);
		}
	}

	function onAction() {
		if (busy) oncancel();
		else onsubmit();
	}
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div {@attach captureRoot} class={['prompt', variant]}>
	<label class="prompt-main">
		{#if !docked}
			<Icon src="/icons/search.svg" />
		{/if}
		<textarea
			{@attach captureTextarea}
			bind:value
			{placeholder}
			aria-label={placeholder}
			rows="1"
			onkeydown={onPromptKey}
			oninput={resize}
			{oncompositionstart}
			{oncompositionend}
		></textarea>
	</label>
	<div class="prompt-tools">
		<div class="prompt-mode-wrap">
			<button
				type="button"
				class="prompt-mode"
				aria-haspopup="menu"
				aria-expanded={menuOpen}
				aria-label="Computer approval: {approvalLabel(approval)}"
				onclick={toggleMenu}
				onkeydown={onModeKey}
			>
				<span>{approvalLabel(approval)}</span>
				<Chevron expanded />
			</button>
			{#if menuOpen}
				<div class={['prompt-menu', docked ? 'up' : 'down']} role="menu" aria-label="Computer approval">
					{#each APPROVAL_MODES as mode, index (mode)}
						<button
							type="button"
							class={['prompt-option', { active: index === activeIndex, selected: mode === approval }]}
							role="menuitem"
							onpointerenter={() => (activeIndex = index)}
							onclick={() => selectMode(mode)}
						>
							{approvalLabel(mode)}
						</button>
					{/each}
				</div>
			{/if}
		</div>
		{#if docked}
			<button
				type="button"
				class={['prompt-action', busy && 'stop']}
				aria-label={busy ? 'Stop' : 'Send'}
				disabled={!busy && !sendReady}
				onclick={onAction}
			>
				{#if busy}
					<Icon src="/icons/stop.svg" color="var(--tone-red)" size={12} />
				{:else}
					<Icon src="/icons/arrow-up.svg" color="var(--button-primary-text)" size={14} />
				{/if}
			</button>
		{/if}
	</div>
</div>
