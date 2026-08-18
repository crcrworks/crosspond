<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import {
		approve,
		bootstrap,
		cancel,
		cycleComputerApproval,
		hideLauncher,
		listHistory,
		loadSettings,
		openSettings,
		reject,
		resetSession,
		revealArtifact,
		revealHistoryArtifact,
		setUiFlags,
		startTask,
		syncLauncherSize
	} from '$lib/api';
	import ActivityLabel from '$lib/components/ActivityLabel.svelte';
	import ApprovalCard from '$lib/components/ApprovalCard.svelte';
	import Button from '$lib/components/Button.svelte';
	import HistoryPanel from '$lib/components/HistoryPanel.svelte';
	import Onboarding from '$lib/components/Onboarding.svelte';
	import ReceiptCard from '$lib/components/ReceiptCard.svelte';
	import TranscriptView from '$lib/components/TranscriptView.svelte';
	import { LauncherSession } from '$lib/session.svelte';
	import { approvalLabel } from '$lib/tools';
	import type { AgentEvent, LauncherShown } from '$lib/types';
	import { onMount } from 'svelte';

	const session = new LauncherSession();
	let textarea: HTMLTextAreaElement | undefined = $state();
	let extraHeight = $state(0);
	let stickToBottom = $state(true);
	let scroller: HTMLDivElement | undefined = $state();

	const chatLayout = $derived(
		session.inConversation && session.overlay === 'none'
	);
	const showInput = $derived(session.overlay !== 'onboarding');
	const expanded = $derived(!session.compact);
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

	// Size follows compact/overlay/badges, not transcript `rev`.
	let appliedCompact = true;

	function resize() {
		const compact = session.compact;
		void setUiFlags(compact, session.composing, session.inConversation);
		if (!compact && !appliedCompact) {
			return;
		}
		appliedCompact = compact;
		const badges = session.overlay === 'onboarding' || chatLayout ? 0 : session.badges.length;
		void syncLauncherSize(compact, badges, extraHeight);
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
			session.applyShown(boot.badges, boot.needs_onboarding, !boot.needs_onboarding, boot.visible);
			if (boot.needs_onboarding) session.enterOnboarding(!boot.needs_onboarding);
			unlistenEvent = await listen<AgentEvent>('agent-event', (event) => {
				session.applyEvent(event.payload);
			});
			unlistenShown = await listen<LauncherShown>('launcher-shown', (event) => {
				const shown = event.payload;
				session.applyShown(shown.badges, shown.onboarding, shown.ready, shown.visible);
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
		const prompt = session.input.trim();
		if (prompt.length === 0) return;
		try {
			const taskId = await startTask(prompt);
			session.beginTask(taskId, prompt);
		} catch (error) {
			session.transcript.pushNotice(String(error));
			session.state = 'failed';
			session.failedMessage = String(error);
			session.bump();
		}
	}

	async function onNew() {
		if (session.overlay === 'onboarding') return;
		session.resetLocal();
		await resetSession();
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

	async function onEscape() {
		if (session.busy) {
			await cancel();
			return;
		}
		if (session.overlay === 'history' && session.historySelected !== null) {
			session.historySelected = null;
			session.bump();
			return;
		}
		if (session.overlay === 'history') {
			session.overlay = 'none';
			session.bump();
			return;
		}
		await hideLauncher();
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
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
		if (event.key === 'ArrowUp' && session.input.length === 0 && session.overlay === 'none') {
			event.preventDefault();
			void onHistory();
		}
	}

	function onPromptKey(event: KeyboardEvent) {
		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			void submit();
		}
	}

	async function continueOnboarding() {
		const loaded = await loadSettings();
		if (loaded.provider_key_stored) {
			session.onboardingReady = true;
			session.onboardingHint = null;
			return;
		}
		session.onboardingHint = 'Add an API key in Settings first.';
		await openSettings();
	}

	function onInput() {
		if (!textarea) return;
		textarea.style.height = 'auto';
		const next = Math.min(textarea.scrollHeight, 160);
		textarea.style.height = `${next}px`;
		extraHeight = Math.max(0, next - 24);
	}
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-full w-full items-start justify-center p-0">
	<div
		class="flex w-full flex-col overflow-hidden rounded-xl border text-[var(--text)] shadow-lg"
		style:background="var(--bg)"
		style:border-color="var(--border)"
		style:min-height={expanded ? '100%' : 'auto'}
		style:height={expanded ? '100%' : 'auto'}
	>
		<div
			class="flex shrink-0 flex-row items-center gap-3 px-4 py-3"
			class:items-end={expanded}
			data-tauri-drag-region
		>
			<div class="size-2 shrink-0 rounded-full bg-[var(--ok)]"></div>
			{#if session.overlay === 'onboarding'}
				<div class="min-w-0 flex-1 text-sm">Welcome to Crosspond</div>
			{:else}
				<textarea
					bind:this={textarea}
					bind:value={session.input}
					placeholder={session.placeholder}
					rows="1"
					class="min-w-0 flex-1 resize-none bg-transparent text-sm outline-none"
					onkeydown={onPromptKey}
					oninput={onInput}
					oncompositionstart={() => (session.composing = true)}
					oncompositionend={() => (session.composing = false)}
				></textarea>
				<Button
					label={approvalLabel(session.computerApproval)}
					onclick={async () => {
						session.computerApproval = await cycleComputerApproval();
					}}
				/>
			{/if}
			{#if session.inConversation && session.overlay !== 'onboarding'}
				<Button label="New" onclick={() => void onNew()} />
			{/if}
			{#if showInput}
				<Button
					label="History"
					onclick={() => void onHistory()}
				/>
			{/if}
			{#if session.busy}
				<Button label="Stop" onclick={() => void cancel()} />
			{/if}
		</div>
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
				class="min-h-0 flex-1 overflow-y-auto px-4 pb-4"
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
						onsettings={() => void openSettings()}
						oncontinue={() => void continueOnboarding()}
						ondone={() => void hideLauncher()}
					/>
				{:else if session.overlay === 'history'}
					<HistoryPanel
						entries={session.history}
						selected={session.historySelected}
						onselect={(index) => (session.historySelected = index)}
						onback={() => (session.historySelected = null)}
						onreveal={(taskId, name) => void revealHistoryArtifact(taskId, name)}
					/>
				{:else}
					<TranscriptView
						{blocks}
						{thinkingLiveIndex}
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
</div>
