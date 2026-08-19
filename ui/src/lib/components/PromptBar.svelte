<script lang="ts">
	import { APPROVAL_MODES, approvalLabel } from '$lib/tools';
	import { listModels, saveEffort, saveSelected } from '$lib/api';
	import { composerExtraHeight } from '$lib/launcher-size';
	import { EFFORTS, effortLabel } from '$lib/models';
	import {
		chipLabel,
		filterCatalog,
		mentionFromKind,
		mentionTrigger,
		type Mention,
		type MentionCatalogItem
	} from '$lib/mentions';
	import type { ComputerApproval, ModelGroup, ReasoningEffort, SelectedModel } from '$lib/types';
	import Chevron from './Chevron.svelte';
	import Icon from './Icon.svelte';

	type PickerStage = 'kinds' | 'app';

	let {
		variant = 'seamless',
		value = $bindable(''),
		textarea = $bindable<HTMLTextAreaElement | undefined>(undefined),
		menuOpen = $bindable(false),
		pickerOpen = $bindable(false),
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
		pickerOpen: boolean;
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
	let appsLoading = $state(false);
	let appsLoadId = 0;
	let composing = $state(false);
	let groups = $state<ModelGroup[]>([]);
	let selected = $state<SelectedModel>({ source: 'default', model: 'gpt-4o-mini' });
	let effort = $state<ReasoningEffort>('medium');
	let modelMenuOpen = $state(false);
	let effortMenuOpen = $state(false);
	let customOpen = $state(false);
	let customModel = $state('');
	let customSource = $state('default');
	const docked = $derived(variant === 'docked');
	const sendReady = $derived(canSubmit && (value.trim().length > 0 || mentions.length > 0));
	const activeStage = $derived(mentionOpen ? stage : null);
	const kindItems = $derived(filterCatalog(activeStage === 'kinds' ? stageQuery : ''));
	const filteredApps = $derived(
		appHits.filter((name) => name.toLowerCase().includes(stageQuery.trim().toLowerCase()))
	);
	const mentionCount = $derived(activeStage === 'kinds' ? kindItems.length : filteredApps.length);
	const chatgptSelected = $derived(selected.source === 'chatgpt');
	const modelButtonLabel = $derived(selected.model || 'Model');
	const showModelMenu = $derived(pickerOpen && modelMenuOpen);
	const showEffortMenu = $derived(pickerOpen && effortMenuOpen && chatgptSelected);
	const showCustom = $derived(pickerOpen && customOpen);

	function closePickers() {
		modelMenuOpen = false;
		effortMenuOpen = false;
		customOpen = false;
		pickerOpen = false;
	}

	async function refreshModels() {
		try {
			const catalog = await listModels();
			groups = catalog.groups;
			selected = catalog.selected;
			effort = catalog.reasoning_effort;
		} catch {
			groups = [];
		}
	}

	async function chooseModel(source: string, model: string) {
		closePickers();
		selected = { source, model };
		try {
			selected = await saveSelected(source, model);
		} catch {
			/* keep local selection */
		}
	}

	function chooseCustom(source: string) {
		modelMenuOpen = false;
		effortMenuOpen = false;
		customSource = source;
		customModel = selected.source === source ? selected.model : '';
		customOpen = true;
		pickerOpen = true;
	}

	async function commitCustom() {
		const model = customModel.trim();
		if (!model) {
			customOpen = false;
			return;
		}
		await chooseModel(customSource, model);
		customOpen = false;
	}

	async function chooseEffort(next: ReasoningEffort) {
		if (!chatgptSelected) return;
		closePickers();
		effort = next;
		try {
			effort = (await saveEffort(next)) as ReasoningEffort;
		} catch {
			/* keep local */
		}
	}

	function toggleModelMenu(event: MouseEvent) {
		event.preventDefault();
		event.stopPropagation();
		closeMentions();
		closeMenu();
		effortMenuOpen = false;
		customOpen = false;
		if (showModelMenu) {
			closePickers();
			return;
		}
		modelMenuOpen = true;
		pickerOpen = true;
		void refreshModels();
	}

	function toggleEffortMenu(event: MouseEvent) {
		event.preventDefault();
		event.stopPropagation();
		if (!chatgptSelected) return;
		closeMentions();
		closeMenu();
		modelMenuOpen = false;
		customOpen = false;
		if (showEffortMenu) {
			closePickers();
			return;
		}
		effortMenuOpen = true;
		pickerOpen = true;
	}

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
		appsLoadId += 1;
		stage = null;
		mentionOpen = false;
		stageQuery = '';
		appHits = [];
		appsLoading = false;
	}

	function openKinds(start: number, query: string) {
		menuOpen = false;
		closePickers();
		stage = 'kinds';
		triggerStart = start;
		stageQuery = query;
		mentionIndex = 0;
		mentionOpen = true;
	}

	function syncTriggerFromValue() {
		if (composing) return;
		if (activeStage === 'app') return;
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

	async function loadApps() {
		const id = (appsLoadId += 1);
		appsLoading = true;
		try {
			const names = await onlistapps();
			if (id !== appsLoadId || !mentionOpen) return;
			appHits = names;
		} catch {
			if (id !== appsLoadId || !mentionOpen) return;
			appHits = [];
		} finally {
			if (id === appsLoadId) appsLoading = false;
		}
	}

	function selectKind(item: MentionCatalogItem) {
		if (item.needsPicker) {
			replaceTrigger('');
			stage = 'app';
			stageQuery = '';
			mentionIndex = 0;
			mentionOpen = true;
			menuOpen = false;
			void loadApps();
			return;
		}
		addMention(mentionFromKind(item.kind));
	}

	function selectActiveMention() {
		if (activeStage === 'kinds') {
			const item = kindItems[mentionIndex];
			if (item) selectKind(item);
			return;
		}
		if (activeStage === 'app') {
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
		if (activeStage === 'app') {
			const cursor = textarea?.selectionStart ?? value.length;
			stageQuery = value.slice(triggerStart, cursor);
			return;
		}
		syncTriggerFromValue();
	}

	function openMenu() {
		closeMentions();
		closePickers();
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
		closePickers();
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

	void refreshModels();
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div {@attach captureRoot} class={['prompt', variant]} data-tauri-drag-region="false">
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
		<div class="prompt-pickers">
			<div class="prompt-mode-wrap picker-left">
				<button
					type="button"
					class="prompt-mode"
					aria-haspopup="menu"
					aria-expanded={showModelMenu}
					aria-label="Model {modelButtonLabel}"
					onclick={toggleModelMenu}
				>
					<span class="picker-label">{modelButtonLabel}</span>
					<Chevron expanded />
				</button>
				{#if showModelMenu}
					<div class={['prompt-menu', 'picker-menu', docked ? 'up' : 'down']} role="menu" aria-label="Models">
						{#each groups as group (group.source)}
							<div class="prompt-group">{group.label}</div>
							{#each group.models as model (group.source + model.id)}
								<button
									type="button"
									class={[
										'prompt-option',
										{
											selected:
												selected.source === group.source && selected.model === model.id
										}
									]}
									role="menuitem"
									onclick={() => void chooseModel(group.source, model.id)}
								>
									{model.label}
								</button>
							{/each}
							<button
								type="button"
								class="prompt-option"
								role="menuitem"
								onclick={() => chooseCustom(group.source)}
							>
								Custom…
							</button>
						{/each}
						{#if groups.length === 0}
							<div class="mention-empty">Add a provider in Settings</div>
						{/if}
					</div>
				{/if}
				{#if showCustom}
					<div class={['prompt-menu', 'picker-menu', docked ? 'up' : 'down']}>
						<input
							class="picker-custom"
							bind:value={customModel}
							placeholder="Model id"
							aria-label="Custom model"
							onkeydown={(event) => {
								if (event.key === 'Enter') {
									event.preventDefault();
									void commitCustom();
								}
								if (event.key === 'Escape') {
									event.preventDefault();
									customOpen = false;
								}
							}}
						/>
						<button type="button" class="prompt-option" onclick={() => void commitCustom()}>
							Use this model
						</button>
					</div>
				{/if}
			</div>
			<div class="prompt-mode-wrap picker-left">
				<button
					type="button"
					class={['prompt-mode', !chatgptSelected && 'disabled']}
					aria-haspopup="menu"
					aria-expanded={showEffortMenu}
					aria-label="Reasoning effort {effortLabel(effort)}"
					disabled={!chatgptSelected}
					title={chatgptSelected ? 'Codex reasoning effort' : 'Effort applies to ChatGPT models'}
					onclick={toggleEffortMenu}
				>
					<span>{effortLabel(effort)}</span>
					<Chevron expanded />
				</button>
				{#if showEffortMenu}
					<div class={['prompt-menu', 'picker-menu', docked ? 'up' : 'down']} role="menu" aria-label="Effort">
						{#each EFFORTS as item (item)}
							<button
								type="button"
								class={['prompt-option', { selected: item === effort }]}
								role="menuitem"
								onclick={() => void chooseEffort(item)}
							>
								{effortLabel(item)}
							</button>
						{/each}
					</div>
				{/if}
			</div>
			{#if !chatgptSelected}
				<span class="picker-note">Effort is Codex only</span>
			{/if}
		</div>
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
			{#if activeStage === 'kinds'}
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
			{:else if activeStage === 'app'}
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
					<div class="mention-empty">{appsLoading ? 'Loading apps…' : 'Type an app name'}</div>
				{/if}
			{/if}
		</div>
	{/if}
</div>
