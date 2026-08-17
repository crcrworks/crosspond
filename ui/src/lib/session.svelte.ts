import type { AgentEvent, ComputerApproval, HistoryItem, Receipt, WindowState } from './types';
import {
	Transcript,
	failedOffersSettings,
	heartbeatStatus,
	type LiveActivity
} from './transcript';

const ASK = 'Ask or do anything...';
const FOLLOW_UP = 'Ask a follow-up...';
const MIN_TOOL_MS = 800;

export class LauncherSession {
	transcript = new Transcript();
	rev = $state(0);
	state = $state<WindowState>('idle');
	input = $state('');
	placeholder = $state(ASK);
	overlay = $state<'none' | 'onboarding' | 'history'>('none');
	onboardingReady = $state(false);
	onboardingHint = $state<string | null>(null);
	badges = $state<string[]>([]);
	currentTask = $state<string | null>(null);
	pendingApproval = $state<{
		id: string;
		title: string;
		description: string;
	} | null>(null);
	artifacts = $state<string[]>([]);
	receipt = $state<Receipt | null>(null);
	activity = $state<LiveActivity>({ kind: 'thinking' });
	computerApproval = $state<ComputerApproval>('manual');
	history = $state<HistoryItem[]>([]);
	historySelected = $state<number | null>(null);
	visible = $state(false);
	composing = $state(false);
	failedMessage = $state<string | null>(null);
	#toolStarts: { name: string; at: number }[] = [];
	#finishTimers = new Map<string, ReturnType<typeof setTimeout>>();

	get inConversation() {
		this.rev;
		return this.state !== 'idle' || !this.transcript.isEmpty;
	}

	get compact() {
		this.rev;
		return !this.inConversation && this.overlay === 'none';
	}

	get busy() {
		return (
			this.state === 'running' ||
			this.state === 'preparing_context' ||
			this.state === 'waiting_approval'
		);
	}

	get heartbeat() {
		this.rev;
		return heartbeatStatus(this.state, this.transcript, this.activity);
	}

	get offerSettings() {
		return this.state === 'failed' && this.failedMessage !== null
			? failedOffersSettings(this.failedMessage)
			: false;
	}

	bump() {
		this.rev += 1;
	}

	resetLocal() {
		for (const timer of this.#finishTimers.values()) clearTimeout(timer);
		this.#finishTimers.clear();
		this.transcript.clear();
		this.state = 'idle';
		this.input = '';
		this.placeholder = ASK;
		this.overlay = 'none';
		this.onboardingHint = null;
		this.currentTask = null;
		this.pendingApproval = null;
		this.artifacts = [];
		this.receipt = null;
		this.activity = { kind: 'thinking' };
		this.historySelected = null;
		this.failedMessage = null;
		this.#toolStarts = [];
		this.bump();
	}

	enterOnboarding(ready: boolean) {
		if (this.busy) return;
		this.overlay = 'onboarding';
		this.onboardingReady = ready;
		this.onboardingHint = null;
		this.bump();
	}

	applyShown(badges: string[], onboarding: boolean, ready: boolean, visible: boolean) {
		this.visible = visible;
		if (!visible) return;
		this.badges = badges;
		if (onboarding) this.enterOnboarding(ready);
		else if (this.overlay === 'onboarding' && ready) this.onboardingReady = true;
	}

	applyEvent(event: AgentEvent) {
		if (event.type === 'connection_tested') {
			if (this.overlay === 'onboarding' && event.ok) {
				this.onboardingReady = true;
				this.onboardingHint = null;
			}
			return;
		}
		if (!('task_id' in event) || this.currentTask !== event.task_id) return;
		switch (event.type) {
			case 'task_started':
				this.state = 'preparing_context';
				this.activity = { kind: 'thinking' };
				break;
			case 'context_collected':
				this.state = 'running';
				this.activity = { kind: 'thinking' };
				break;
			case 'assistant_delta':
				this.transcript.pushText(event.text);
				this.state = 'running';
				this.activity = { kind: 'writing' };
				break;
			case 'reasoning_delta':
				this.transcript.pushReasoning(event.text);
				this.state = 'running';
				this.activity = { kind: 'thinking' };
				break;
			case 'tool_started':
				this.#toolStarts.push({ name: event.tool, at: Date.now() });
				this.transcript.startTool(event.tool, event.summary);
				this.activity = { kind: 'tool', name: event.tool };
				break;
			case 'tool_finished':
				this.finishToolSoon(event.tool);
				break;
			case 'artifact_created':
				this.artifacts = [...this.artifacts, event.display_name];
				break;
			case 'approval_required':
				this.pendingApproval = {
					id: event.approval_id,
					title: event.title,
					description: event.description
				};
				this.state = 'waiting_approval';
				break;
			case 'task_completed':
				if (!this.transcript.hasAssistantTextSinceLastUser() && event.summary.trim() !== '') {
					this.transcript.pushText(event.summary);
				}
				this.receipt = event.receipt;
				this.state = 'completed';
				this.settle();
				break;
			case 'task_failed':
				this.transcript.pushNotice(event.message);
				this.failedMessage = event.message;
				this.state = 'failed';
				this.settle();
				break;
			case 'task_cancelled':
				this.state = 'cancelled';
				this.settle();
				break;
		}
		this.bump();
	}

	beginTask(taskId: string, prompt: string) {
		this.currentTask = taskId;
		this.transcript.pushUser(prompt);
		this.artifacts = [];
		this.receipt = null;
		this.overlay = 'none';
		this.pendingApproval = null;
		this.activity = { kind: 'thinking' };
		this.state = 'preparing_context';
		this.input = '';
		this.placeholder = FOLLOW_UP;
		this.failedMessage = null;
		this.bump();
	}

	private settle() {
		this.pendingApproval = null;
		this.transcript.finishRunningTools();
		this.#toolStarts = [];
		for (const timer of this.#finishTimers.values()) clearTimeout(timer);
		this.#finishTimers.clear();
	}

	private finishToolSoon(tool: string) {
		const idx = this.#toolStarts.findLastIndex((item) => item.name === tool);
		const started = idx >= 0 ? this.#toolStarts.splice(idx, 1)[0] : this.#toolStarts.pop();
		const elapsed = started ? Date.now() - started.at : MIN_TOOL_MS;
		if (elapsed >= MIN_TOOL_MS) {
			this.transcript.finishTool(tool);
			this.activity = { kind: 'preparing' };
			this.bump();
			return;
		}
		const wait = MIN_TOOL_MS - elapsed;
		const taskId = this.currentTask;
		const timer = setTimeout(() => {
			this.#finishTimers.delete(tool);
			if (this.currentTask !== taskId) return;
			this.transcript.finishTool(tool);
			this.activity = { kind: 'preparing' };
			this.bump();
		}, wait);
		this.#finishTimers.set(tool, timer);
	}
}
