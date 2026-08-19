<script lang="ts">
	import { APPROVAL_MODES, approvalLabel } from '$lib/tools';
	import { listModels, saveEffort, saveSelected } from '$lib/api';
	import { listen } from '@tauri-apps/api/event';
	import { onMount } from 'svelte';
	import { composerExtraHeight } from '$lib/launcher-size';
	import {
		CUSTOM_MODEL,
		EFFORTS,
		effortLabel,
		isCustomModelOption,
		modelOptionValue,
		parseModelOption
	} from '$lib/models';
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
	let customSource = $state<string | null>(null);
	let customModel = $state('');
	const docked = $derived(variant === 'docked');
	const sendReady = $derived(canSubmit && (value.trim().length > 0 || mentions.length > 0));
	const activeStage = $derived(mentionOpen ? stage : null);
	const kindItems = $derived(filterCatalog(activeStage === 'kinds' ? stageQuery : ''));
	const filteredApps = $derived(
		appHits.filter((name) => name.toLowerCase().includes(stageQuery.trim().toLowerCase()))
	);
	const mentionCount = $derived(activeStage === 'kinds' ? kindItems.length : filteredApps.length);
	const chatgptSelected = $derived(selected.source === 'chatgpt');
	const modelSelectValue = $derived(modelOptionValue(selected.source, selected.model));
	const showCustom = $derived(pickerOpen && customSource !== null);
	const selectedInList = $derived(
		groups.some(
			(group) =>
				group.source === selected.source &&
				group.models.some((model) => model.id === selected.model)
		)
	);
	let pendingSaves = 0;

	function closePickers() {
		customSource = null;
		pickerOpen = false;
	}

	async function refreshModels() {
		try {
			const catalog = await listModels();
			groups = catalog.groups;
			if (pendingSaves === 0) {
				selected = catalog.selected;
				effort = catalog.reasoning_effort;
			}
		} catch {
			groups = [];
		}
	}

	async function chooseModel(source: string, model: string) {
		closePickers();
		pendingSaves += 1;
		selected = { source, model };
		try {
			selected = await saveSelected(source, model);
		} catch {
			/* keep local selection */
		} finally {
			pendingSaves -= 1;
		}
	}

	async function commitCustom() {
		const model = customModel.trim();
		const source = customSource;
		if (!model || !source) {
			closePickers();
			return;
		}
		await chooseModel(source, model);
	}

	async function chooseEffort(next: ReasoningEffort) {
		if (!chatgptSelected) return;
		pendingSaves += 1;
		effort = next;
		try {
			effort = (await saveEffort(next)) as ReasoningEffort;
		} catch {
			/* keep local */
		} finally {
			pendingSaves -= 1;
		}
	}

	function onModelActivate() {
		customSource = null;
		pickerOpen = true;
		closeMentions();
		menuOpen = false;
	}

	function onModelChange(event: Event) {
		const value = (event.currentTarget as HTMLSelectElement).value;
		const parsed = parseModelOption(value);
		if (isCustomModelOption(parsed.model)) {
			customSource = parsed.source;
			customModel = selected.source === parsed.source ? selected.model : '';
			pickerOpen = true;
			return;
		}
		void chooseModel(parsed.source, parsed.model);
	}

	function onEffortChange(event: Event) {
		void chooseEffort((event.currentTarget as HTMLSelectElement).value as ReasoningEffort);
	}

	function onApprovalChange(event: Event) {
		const mode = (event.currentTarget as HTMLSelectElement).value as ComputerApproval;
		if (mode !== approval) onapproval(mode);
	}

	function onPickerBlur() {
		if (customSource !== null) return;
		pickerOpen = false;
	}

	function focusCustom(node: HTMLInputElement) {
		queueMicrotask(() => node.focus());
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

	function onWindowPointerDown(event: PointerEvent) {
		if (!root) return;
		if (root.contains(event.target as Node)) return;
		closeMentions();
		if (customSource !== null) {
			void commitCustom();
			return;
		}
		closePickers();
	}

	function onAction() {
		if (busy) oncancel();
		else if (mentionOpen) selectActiveMention();
		else onsubmit();
	}

	onMount(() => {
		void refreshModels();
		let unlistenChanged: (() => void) | undefined;
		let unlistenLogin: (() => void) | undefined;
		let unlistenShown: (() => void) | undefined;
		void listen('models-changed', () => {
			void refreshModels();
		}).then((fn) => {
			unlistenChanged = fn;
		});
		void listen<{ ok: boolean }>('chatgpt-login', (event) => {
			if (event.payload.ok) void refreshModels();
		}).then((fn) => {
			unlistenLogin = fn;
		});
		void listen('launcher-shown', () => {
			void refreshModels();
		}).then((fn) => {
			unlistenShown = fn;
		});
		return () => {
			unlistenChanged?.();
			unlistenLogin?.();
			unlistenShown?.();
		};
	});
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

{#snippet approvalPicker()}
	<div class="prompt-mode-wrap picker-native">
		<select
			class="prompt-mode picker-select picker-approval"
			aria-label="Computer approval: {approvalLabel(approval)}"
			value={approval}
			onfocus={() => (menuOpen = true)}
			onblur={() => (menuOpen = false)}
			onchange={onApprovalChange}
		>
			{#each APPROVAL_MODES as mode (mode)}
				<option value={mode}>{approvalLabel(mode)}</option>
			{/each}
		</select>
		<span class="picker-caret">
			<Chevron expanded />
		</span>
	</div>
{/snippet}

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
			<div class="prompt-mode-wrap picker-native">
				{#if showCustom}
					<input
						{@attach focusCustom}
						class="picker-custom-inline"
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
								closePickers();
							}
						}}
						onblur={() => {
							if (customModel.trim()) void commitCustom();
							else closePickers();
						}}
					/>
				{:else}
					<select
						class="prompt-mode picker-select"
						aria-label="Model {selected.model}"
						value={modelSelectValue}
						onfocus={onModelActivate}
						onmousedown={onModelActivate}
						onblur={onPickerBlur}
						onchange={onModelChange}
					>
						{#if !selectedInList}
							<option value={modelSelectValue}>{selected.model || 'Model'}</option>
						{/if}
						{#each groups as group (group.source)}
							<optgroup label={group.label}>
								{#each group.models as model (group.source + model.id)}
									<option value={modelOptionValue(group.source, model.id)}>{model.label}</option>
								{/each}
								<option value={modelOptionValue(group.source, CUSTOM_MODEL)}>Custom…</option>
							</optgroup>
						{/each}
					</select>
					<span class="picker-caret">
						<Chevron expanded />
					</span>
				{/if}
			</div>
			{#if chatgptSelected}
				<div class="prompt-mode-wrap picker-native">
					<select
						class="prompt-mode picker-select picker-effort"
						aria-label="Reasoning effort {effortLabel(effort)}"
						title="Codex reasoning effort"
						value={effort}
						onfocus={() => (pickerOpen = true)}
						onblur={onPickerBlur}
						onchange={onEffortChange}
					>
						{#each EFFORTS as item (item)}
							<option value={item}>{effortLabel(item)}</option>
						{/each}
					</select>
					<span class="picker-caret">
						<Chevron expanded />
					</span>
				</div>
			{/if}
			{#if !docked}
				<div class="prompt-pickers-end">
					{@render approvalPicker()}
				</div>
			{/if}
		</div>
	</div>
	{#if docked}
		<div class="prompt-tools">
			{@render approvalPicker()}
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
		</div>
	{/if}
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
