<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import {
		approve,
		bootstrap,
		cancel,
		hideLauncher,
		listHistory,
		loadSettings,
		openConversation,
		openSettings,
		reject,
		resetSession,
		revealArtifact,
		listMentionApps,
		setComputerApproval,
		setUiFlags,
		startTask,
		syncLauncherSize
	} from '$lib/api';
	import ActivityLabel from '$lib/components/ActivityLabel.svelte';
	import ApprovalCard from '$lib/components/ApprovalCard.svelte';
	import Button from '$lib/components/Button.svelte';
	import ConversationHeader from '$lib/components/ConversationHeader.svelte';
	import HistoryPanel from '$lib/components/HistoryPanel.svelte';
	import Onboarding from '$lib/components/Onboarding.svelte';
	import PromptBar from '$lib/components/PromptBar.svelte';
	import ReceiptCard from '$lib/components/ReceiptCard.svelte';
	import TranscriptView from '$lib/components/TranscriptView.svelte';
	import { LauncherSession } from '$lib/session.svelte';
	import { firstUserTitle } from '$lib/transcript';
	import { shouldSyncLauncherSize } from '$lib/launcher-size';
	import {
		MENTION_CHIP_ROW,
		MENTION_MENU_HEIGHT,
		displayPrompt,
		mergeMentions,
		takeInlineMentions
	} from '$lib/mentions';
	import type { AgentEvent, ComputerApproval, LauncherShown } from '$lib/types';
	import { onMount } from 'svelte';

	const session = new LauncherSession();
	let textarea: HTMLTextAreaElement | undefined = $state();
	let textExtra = $state(0);
	let modeMenuOpen = $state(false);
	let mentionOpen = $state(false);
	let stickToBottom = $state(true);
	let scroller: HTMLDivElement | undefined = $state();
	let hotkeyTokens = $state<string[]>(['Option', 'Space']);

	const chatLayout = $derived(
		session.inConversation && session.overlay === 'none'
	);
	const expanded = $derived(!session.compact);
	const extraHeight = $derived(
		textExtra +
			(session.mentions.length > 0 ? MENTION_CHIP_ROW : 0) +
			(session.compact && mentionOpen ? MENTION_MENU_HEIGHT : 0) +
			(session.compact && modeMenuOpen && !mentionOpen ? 120 : 0)
	);
	const canSubmit = $derived(
		session.state === 'idle' ||
			session.state === 'completed' ||
			session.state === 'failed' ||
			session.state === 'cancelled'
	);
	const dockPrompt = $derived(expanded && session.overlay !== 'onboarding');
	const blocks = $derived.by(() => {
		session.rev;
		return session.transcript.snapshot();
	});
	const thinkingLiveIndex = $derived.by(() => {
		session.rev;
		return session.state === 'running' && session.activity.kind === 'thinking'
			? session.transcript.liveThinkingIndex()
			: null;
	});
	const showHeader = $derived(session.overlay !== 'onboarding');
	const liveTitle = $derived.by(() => {
		session.rev;
		return firstUserTitle(session.transcript.blocks());
	});
	const historyTabs = $derived(
		session.history.filter((item) => item.id !== session.currentConversation)
	);

	// Size follows compact/overlay/badges, not transcript `rev`.
	let appliedCompact = true;

	function resetComposerSize() {
		modeMenuOpen = false;
		mentionOpen = false;
		textExtra = 0;
		if (textarea) textarea.style.height = 'auto';
	}

	function resize() {
		const compact = session.compact;
		const composing = session.composing || mentionOpen || modeMenuOpen;
		const inConversation = session.inConversation;
		const badges = session.overlay === 'onboarding' || chatLayout ? 0 : session.badges.length;
		const extra = extraHeight;
		void setUiFlags(compact, composing, inConversation);
		if (!shouldSyncLauncherSize(compact, appliedCompact)) {
			return;
		}
		appliedCompact = compact;
		void syncLauncherSize(compact, badges, extra);
	}

	$effect(() => {
		resize();
	});

	$effect(() => {
		session.rev;
		if (stickToBottom && scroller) {
			scroller.scrollTop = scroller.scrollHeight;
		}
	});

	onMount(() => {
		let unlistenEvent: (() => void) | undefined;
		let unlistenShown: (() => void) | undefined;
		void (async () => {
			const boot = await bootstrap();
			session.computerApproval = boot.computer_approval;
			hotkeyTokens = boot.launcher_hotkey.tokens;
			session.applyShown(boot.badges, boot.needs_onboarding, !boot.needs_onboarding, boot.visible);
			if (boot.needs_onboarding) session.enterOnboarding(!boot.needs_onboarding);
			void refreshHistory();
			unlistenEvent = await listen<AgentEvent>('agent-event', (event) => {
				session.applyEvent(event.payload);
				if (
					event.payload.type === 'task_completed' ||
					event.payload.type === 'task_failed' ||
					event.payload.type === 'task_cancelled'
				) {
					void refreshHistory();
				}
			});
			unlistenShown = await listen<LauncherShown>('launcher-shown', (event) => {
				const shown = event.payload;
				hotkeyTokens = shown.launcher_hotkey.tokens;
				session.applyShown(shown.badges, shown.onboarding, shown.ready, shown.visible);
				void refreshHistory();
				if (shown.visible) queueMicrotask(() => textarea?.focus());
			});
			queueMicrotask(() => textarea?.focus());
		})();
		return () => {
			unlistenEvent?.();
			unlistenShown?.();
		};
	});

	async function submit() {
		if (session.overlay === 'onboarding' || session.composing) return;
		if (
			session.state !== 'idle' &&
			session.state !== 'completed' &&
			session.state !== 'failed' &&
			session.state !== 'cancelled'
		) {
			return;
		}
		const taken = takeInlineMentions(session.input);
		const mentions = mergeMentions(session.mentions, taken.mentions);
		const prompt = taken.prompt;
		if (prompt.length === 0 && mentions.length === 0) return;
		try {
			const started = await startTask(prompt, mentions);
			session.beginTask(
				started.task_id,
				started.conversation_id,
				displayPrompt(mentions, prompt)
			);
			resetComposerSize();
		} catch (error) {
			session.transcript.pushNotice(String(error));
			session.state = 'failed';
			session.failedMessage = String(error);
			session.bump();
		}
	}

	async function refreshHistory() {
		session.history = await listHistory();
	}

	async function onNew() {
		if (session.overlay === 'onboarding') return;
		session.resetLocal();
		resetComposerSize();
		await resetSession();
		await refreshHistory();
		textarea?.focus();
	}

	async function onHistory() {
		if (session.busy || session.overlay === 'onboarding') return;
		if (session.overlay === 'history') {
			session.overlay = 'none';
			session.historySelected = null;
			session.bump();
			return;
		}
		session.history = await listHistory();
		session.overlay = 'history';
		session.historySelected = null;
		session.bump();
	}

	async function openPast(id: string) {
		if (session.busy) return;
		if (id === session.currentConversation) {
			session.overlay = 'none';
			session.historySelected = null;
			session.bump();
			return;
		}
		try {
			const view = await openConversation(id);
			session.restoreConversation(view);
			resetComposerSize();
			await refreshHistory();
			textarea?.focus();
		} catch (error) {
			session.transcript.pushNotice(String(error));
			session.state = 'failed';
			session.failedMessage = String(error);
			session.overlay = 'none';
			session.bump();
		}
	}

	async function onEscape() {
		if (session.busy) {
			await cancel();
			return;
		}
		if (session.overlay === 'history') {
			session.overlay = 'none';
			session.historySelected = null;
			session.bump();
			return;
		}
		await hideLauncher();
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			if (modeMenuOpen) {
				modeMenuOpen = false;
				return;
			}
			if (mentionOpen) {
				mentionOpen = false;
				return;
			}
			void onEscape();
			return;
		}
		if ((event.metaKey || event.ctrlKey) && event.key === ',') {
			event.preventDefault();
			void openSettings();
			return;
		}
		if ((event.metaKey || event.ctrlKey) && (event.key === 'n' || event.key === 't')) {
			event.preventDefault();
			void onNew();
			return;
		}
		if ((event.metaKey || event.ctrlKey) && event.key === 'w') {
			event.preventDefault();
			void hideLauncher();
			return;
		}
		if (event.key === 'ArrowUp' && !modeMenuOpen && !mentionOpen && session.input.length === 0 && session.overlay === 'none' && session.mentions.length === 0) {
			event.preventDefault();
			void onHistory();
		}
	}

	async function continueOnboarding() {
		const loaded = await loadSettings();
		if (loaded.provider_ready) {
			session.onboardingReady = true;
			session.onboardingHint = null;
			return;
		}
		session.onboardingHint = 'Sign in with ChatGPT or add an API key in Settings first.';
		await openSettings();
	}

	async function onApproval(mode: ComputerApproval) {
		session.computerApproval = await setComputerApproval(mode);
	}
</script>

<svelte:window onkeydown={onKeydown} />

<div class={['launcher', dockPrompt && 'dock-prompt']}>
	{#if showHeader}
		<ConversationHeader
			liveTitle={session.inConversation ? (liveTitle ?? 'Chat') : null}
			liveActive={session.overlay === 'none' && session.inConversation}
			entries={historyTabs}
			selectedId={null}
			onnew={() => void onNew()}
			onlive={() => {
				session.overlay = 'none';
				session.historySelected = null;
				session.bump();
			}}
			onselect={(id) => void openPast(id)}
		/>
	{/if}
	{#if session.overlay === 'onboarding'}
		<div class="shrink-0 px-4 py-3 text-sm">Welcome to Crosspond</div>
	{:else}
		<div
			class={[
				'prompt-slot',
				expanded ? 'docked' : 'seamless',
				!expanded && session.badges.length === 0 && !modeMenuOpen && !mentionOpen && 'fill'
			]}
			data-tauri-drag-region
		>
			<PromptBar
				variant={expanded ? 'docked' : 'seamless'}
				bind:value={session.input}
				bind:textarea
				bind:menuOpen={modeMenuOpen}
				bind:mentionOpen
				bind:mentions={session.mentions}
				placeholder={session.placeholder}
				approval={session.computerApproval}
				busy={session.busy}
				{canSubmit}
				onsubmit={() => void submit()}
				oncancel={() => void cancel()}
				onapproval={(mode) => void onApproval(mode)}
				ongrow={(extra) => (textExtra = extra)}
				oncompositionstart={() => (session.composing = true)}
				oncompositionend={() => (session.composing = false)}
				onlistapps={() => listMentionApps()}
			/>
		</div>
	{/if}
	{#if session.overlay !== 'onboarding' && !chatLayout && session.badges.length > 0}
		<div class="px-4 pb-2 text-xs text-[var(--muted)]">
			{#each session.badges as line (line)}
				<div>{line}</div>
			{/each}
		</div>
	{/if}
	{#if expanded}
		<div
			bind:this={scroller}
			class="transcript-pane min-h-0 flex-1 overflow-y-auto px-4 pt-3 pb-2"
			onscroll={() => {
				if (!scroller) return;
				const distance = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight;
				stickToBottom = distance < 64;
			}}
		>
			{#if session.overlay === 'onboarding'}
				<Onboarding
					ready={session.onboardingReady}
					hint={session.onboardingHint}
					{hotkeyTokens}
					onsettings={() => void openSettings()}
					oncontinue={() => void continueOnboarding()}
					ondone={() => void hideLauncher()}
				/>
			{:else if session.overlay === 'history'}
				<HistoryPanel
					entries={session.history}
					onselect={(id) => void openPast(id)}
				/>
			{:else}
				<TranscriptView
					{blocks}
					{thinkingLiveIndex}
					preparing={session.state === 'running' && session.activity.kind === 'preparing'}
					ontoggle={(index) => {
						session.transcript.toggle(index);
						session.bump();
					}}
					ontogglestep={(block, step) => {
						session.transcript.toggleStep(block, step);
						session.bump();
					}}
				/>
				{#if session.receipt}
					<ReceiptCard
						receipt={session.receipt}
						names={session.artifacts}
						onreveal={(name) => void revealArtifact(name)}
					/>
				{/if}
				{#if session.heartbeat}
					<div class="w-full overflow-hidden pt-2 text-sm">
						<ActivityLabel text={session.heartbeat} running />
					</div>
				{/if}
				{#if session.offerSettings}
					<div class="pt-2">
						<Button label="Open Settings" onclick={() => void openSettings()} />
					</div>
				{/if}
				{#if session.pendingApproval}
					<ApprovalCard
						title={session.pendingApproval.title}
						description={session.pendingApproval.description}
						onallow={() => {
							const id = session.pendingApproval?.id;
							session.pendingApproval = null;
							session.state = 'running';
							session.bump();
							if (id) void approve(id);
						}}
						oncancel={() => {
							const id = session.pendingApproval?.id;
							session.pendingApproval = null;
							session.state = 'running';
							session.bump();
							if (id) void reject(id);
						}}
					/>
				{/if}
			{/if}
		</div>
	{/if}
</div>

