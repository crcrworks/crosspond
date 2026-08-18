<script lang="ts">
	import { APPROVAL_MODES, approvalLabel } from '$lib/tools';
	import { composerExtraHeight } from '$lib/launcher-size';
	import {
		chipLabel,
		filterCatalog,
		mentionFromKind,
		mentionTrigger,
		type Mention,
		type MentionCatalogItem
	} from '$lib/mentions';
	import type { ComputerApproval } from '$lib/types';
	import Chevron from './Chevron.svelte';
	import Icon from './Icon.svelte';

	type PickerStage = 'kinds' | 'app';

	let {
		variant = 'seamless',
		value = $bindable(''),
		textarea = $bindable<HTMLTextAreaElement | undefined>(undefined),
		menuOpen = $bindable(false),
		mentionOpen = $bindable(false),
		mentions = $bindable<Mention[]>([]),
		placeholder,
		approval,
		busy = false,
		canSubmit = true,
		onsubmit,
		oncancel,
		onapproval,
		ongrow,
		oncompositionstart,
		oncompositionend,
		onlistapps
	}: {
		variant?: 'seamless' | 'docked';
		value: string;
		textarea?: HTMLTextAreaElement;
		menuOpen: boolean;
		mentionOpen: boolean;
		mentions: Mention[];
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
		onlistapps: () => Promise<string[]>;
	} = $props();

	let root: HTMLDivElement | undefined = $state();
	let activeIndex = $state(0);
	let mentionIndex = $state(0);
	let stage = $state<PickerStage | null>(null);
	let triggerStart = $state(0);
	let stageQuery = $state('');
	let appHits = $state<string[]>([]);
	let composing = $state(false);
	const docked = $derived(variant === 'docked');
	const sendReady = $derived(canSubmit && (value.trim().length > 0 || mentions.length > 0));
	const kindItems = $derived(filterCatalog(stage === 'kinds' ? stageQuery : ''));
	const filteredApps = $derived(
		appHits.filter((name) => name.toLowerCase().includes(stageQuery.trim().toLowerCase()))
	);
	const mentionCount = $derived(stage === 'kinds' ? kindItems.length : filteredApps.length);

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

	function closeMentions() {
		stage = null;
		mentionOpen = false;
		stageQuery = '';
		appHits = [];
	}

	function openKinds(start: number, query: string) {
		menuOpen = false;
		stage = 'kinds';
		triggerStart = start;
		stageQuery = query;
		mentionIndex = 0;
		mentionOpen = true;
	}

	function syncTriggerFromValue() {
		if (composing) return;
		if (stage === 'app') return;
		const cursor = textarea?.selectionStart ?? value.length;
		const trigger = mentionTrigger(value, cursor);
		if (!trigger) {
			if (stage === 'kinds') closeMentions();
			return;
		}
		openKinds(trigger.start, trigger.query);
	}

	function replaceTrigger(next: string) {
		const before = value.slice(0, triggerStart);
		const cursor = textarea?.selectionStart ?? value.length;
		const after = value.slice(cursor);
		value = `${before}${next}${after}`;
		queueMicrotask(() => {
			const pos = before.length + next.length;
			textarea?.setSelectionRange(pos, pos);
			resize();
		});
	}

	function addMention(mention: Mention) {
		mentions = [...mentions, mention];
		replaceTrigger('');
		closeMentions();
		queueMicrotask(() => textarea?.focus());
	}

	function removeMention(index: number) {
		mentions = mentions.filter((_, item) => item !== index);
		queueMicrotask(() => textarea?.focus());
	}

	function selectKind(item: MentionCatalogItem) {
		if (item.needsPicker) {
			replaceTrigger('');
			stage = 'app';
			stageQuery = '';
			mentionIndex = 0;
			mentionOpen = true;
			menuOpen = false;
			void onlistapps()
				.then((names) => {
					appHits = names;
				})
				.catch(() => {
					appHits = [];
				});
			return;
		}
		addMention(mentionFromKind(item.kind));
	}

	function selectActiveMention() {
		if (stage === 'kinds') {
			const item = kindItems[mentionIndex];
			if (item) selectKind(item);
			return;
		}
		if (stage === 'app') {
			const name = filteredApps[mentionIndex] ?? stageQuery.trim();
			if (name) addMention({ kind: 'app', name });
		}
	}

	function onPromptKey(event: KeyboardEvent) {
		if (event.key === 'Escape' && mentionOpen) {
			event.preventDefault();
			event.stopPropagation();
			closeMentions();
			return;
		}
		if (mentionOpen && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
			event.preventDefault();
			const count = Math.max(mentionCount, 1);
			mentionIndex =
				event.key === 'ArrowDown'
					? (mentionIndex + 1) % count
					: (mentionIndex - 1 + count) % count;
			return;
		}
		if (mentionOpen && event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			selectActiveMention();
			return;
		}
		if (event.key === 'Backspace' && value.length === 0 && mentions.length > 0) {
			event.preventDefault();
			removeMention(mentions.length - 1);
			return;
		}
		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			onsubmit();
		}
	}

	function onPromptInput() {
		resize();
		if (stage === 'app') {
			const cursor = textarea?.selectionStart ?? value.length;
			stageQuery = value.slice(triggerStart, cursor);
			return;
		}
		syncTriggerFromValue();
	}

	function openMenu() {
		closeMentions();
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
		if (!root) return;
		if (root.contains(event.target as Node)) return;
		closeMenu();
		closeMentions();
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
		else if (mentionOpen) selectActiveMention();
		else onsubmit();
	}

	$effect(() => {
		if (!mentionOpen && stage !== null) {
			stage = null;
			stageQuery = '';
			appHits = [];
		}
	});
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div {@attach captureRoot} class={['prompt', variant]}>
	<div class="prompt-stack">
		{#if mentions.length > 0}
			<div class="prompt-chips">
				{#each mentions as mention, index (chipLabel(mention) + index)}
					<button
						type="button"
						class="prompt-chip"
						aria-label="Remove {chipLabel(mention)}"
						onclick={() => removeMention(index)}
					>
						{chipLabel(mention)}
					</button>
				{/each}
			</div>
		{/if}
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
				oninput={onPromptInput}
				oncompositionstart={() => {
					composing = true;
					oncompositionstart();
				}}
				oncompositionend={() => {
					composing = false;
					oncompositionend();
					syncTriggerFromValue();
				}}
			></textarea>
		</label>
	</div>
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
	{#if mentionOpen}
		<div
			class={['prompt-menu', 'mention-menu', docked ? 'up' : 'down']}
			role="listbox"
			aria-label="Mentions"
		>
			{#if stage === 'kinds'}
				{#each kindItems as item, index (item.kind)}
					<button
						type="button"
						class={['prompt-option', 'mention-option', { active: index === mentionIndex }]}
						role="option"
						aria-selected={index === mentionIndex}
						onpointerenter={() => (mentionIndex = index)}
						onclick={() => selectKind(item)}
					>
						<span class="mention-token">@{item.token}</span>
						<span class="mention-desc">{item.description}</span>
					</button>
				{/each}
				{#if kindItems.length === 0}
					<div class="mention-empty">No matches</div>
				{/if}
			{:else if stage === 'app'}
				{#each filteredApps as name, index (name)}
					<button
						type="button"
						class={['prompt-option', 'mention-option', { active: index === mentionIndex }]}
						role="option"
						aria-selected={index === mentionIndex}
						onpointerenter={() => (mentionIndex = index)}
						onclick={() => addMention({ kind: 'app', name })}
					>
						<span class="mention-token">{name}</span>
					</button>
				{/each}
				{#if filteredApps.length === 0}
					<div class="mention-empty">Type an app name</div>
				{/if}
			{/if}
		</div>
	{/if}
</div>
