import { toolActivityLabel, toolDoneLabel, toolVisual, type ToolTone } from './tools';

export type ToolLine = {
	name: string;
	summary: string;
	running: boolean;
};

export type WorkStep =
	| { kind: 'thinking'; text: string; expanded: boolean; startedAt: number; durationMs: number | null }
	| { kind: 'tool'; tool: ToolLine };

export type TranscriptBlock =
	| { kind: 'user'; text: string }
	| {
			kind: 'work';
			steps: WorkStep[];
			expanded: boolean;
			startedAt: number;
			workedMs: number | null;
	  }
	| { kind: 'text'; text: string };

export type LiveActivity =
	| { kind: 'thinking' }
	| { kind: 'preparing' }
	| { kind: 'writing' }
	| { kind: 'tool'; name: string };

export class Transcript {
	#blocks: TranscriptBlock[] = [];

	blocks(): TranscriptBlock[] {
		return this.#blocks;
	}

	/** Immutable copy so Svelte 5 sees stream updates instead of a mutated array. */
	snapshot(): TranscriptBlock[] {
		return this.#blocks.map((block) => {
			if (block.kind === 'user' || block.kind === 'text') {
				return { kind: block.kind, text: block.text };
			}
			return {
				kind: 'work',
				expanded: block.expanded,
				startedAt: block.startedAt,
				workedMs: block.workedMs,
				steps: block.steps.map((step) => {
					if (step.kind === 'tool') {
						return { kind: 'tool', tool: { ...step.tool } };
					}
					return { ...step };
				})
			};
		});
	}

	clear() {
		this.#blocks = [];
	}

	get isEmpty() {
		return this.#blocks.length === 0;
	}

	hasAssistantTextSinceLastUser(): boolean {
		for (let i = this.#blocks.length - 1; i >= 0; i -= 1) {
			const block = this.#blocks[i];
			if (block.kind === 'user') return false;
			if (block.kind === 'text' && block.text.trim() !== '') return true;
		}
		return false;
	}

	pushUser(text: string) {
		const trimmed = text.trim();
		if (trimmed.length === 0) return;
		this.sealOpenWork();
		this.#blocks.push({ kind: 'user', text: trimmed });
	}

	pushReasoning(delta: string) {
		if (delta.length === 0) return;
		const idx = this.reopenWork();
		if (idx !== null) {
			const block = this.#blocks[idx];
			if (block.kind === 'work') {
				const last = block.steps.at(-1);
				if (last?.kind === 'thinking' && last.durationMs === null) {
					last.text += delta;
					return;
				}
				if (delta.trim().length === 0) return;
				freezeThinkingSteps(block.steps);
				block.steps.push(thinkingStep(delta));
				return;
			}
		}
		if (delta.trim().length === 0) return;
		this.#blocks.push(openWork([thinkingStep(delta)]));
	}

	pushText(delta: string) {
		if (delta.length === 0) return;
		this.freezeOpenThinking();
		const last = this.#blocks.at(-1);
		if (last?.kind === 'text') {
			last.text += delta;
			return;
		}
		const trimmed = delta.trimStart();
		if (trimmed.length === 0) return;
		this.sealOpenWork();
		this.#blocks.push({ kind: 'text', text: trimmed });
	}

	pushNotice(message: string) {
		if (message.length === 0) return;
		this.sealOpenWork();
		this.#blocks.push({ kind: 'text', text: message });
	}

	startTool(name: string, summary: string) {
		this.freezeOpenThinking();
		const line: ToolLine = { name, summary, running: true };
		const idx = this.reopenWork();
		if (idx !== null) {
			const block = this.#blocks[idx];
			if (block.kind === 'work') {
				block.steps.push({ kind: 'tool', tool: line });
				return;
			}
		}
		this.#blocks.push(openWork([{ kind: 'tool', tool: line }]));
	}

	finishTool(name: string) {
		const idx = this.latestWorkInTurn();
		if (idx === null) return;
		const block = this.#blocks[idx];
		if (block.kind !== 'work') return;
		const named = [...block.steps].reverse().find(
			(step) => step.kind === 'tool' && step.tool.running && step.tool.name === name
		);
		if (named?.kind === 'tool') {
			named.tool.running = false;
			return;
		}
		const anyRunning = [...block.steps]
			.reverse()
			.find((step) => step.kind === 'tool' && step.tool.running);
		if (anyRunning?.kind === 'tool') {
			anyRunning.tool.running = false;
		}
	}

	finishRunningTools() {
		for (const block of this.#blocks) {
			if (block.kind !== 'work') continue;
			for (const step of block.steps) {
				if (step.kind === 'tool') step.tool.running = false;
			}
		}
		this.sealOpenWork();
	}

	sealOpenWork() {
		this.freezeOpenThinking();
		const idx = this.openWorkIndex();
		if (idx === null) return;
		const block = this.#blocks[idx];
		if (block.kind !== 'work') return;
		block.workedMs = Date.now() - block.startedAt;
		block.expanded = false;
	}

	toggle(index: number) {
		const block = this.#blocks[index];
		if (block?.kind === 'work') block.expanded = !block.expanded;
	}

	toggleStep(blockIndex: number, step: number) {
		const block = this.#blocks[blockIndex];
		if (block?.kind !== 'work') return;
		const item = block.steps[step];
		if (item?.kind === 'thinking') item.expanded = !item.expanded;
	}

	liveThinkingIndex(): number | null {
		const idx = this.openWorkIndex();
		if (idx === null) return null;
		const block = this.#blocks[idx];
		if (block.kind !== 'work') return null;
		const last = block.steps.at(-1);
		return last?.kind === 'thinking' && last.durationMs === null ? idx : null;
	}

	runningTool(): string | null {
		for (let i = this.#blocks.length - 1; i >= 0; i -= 1) {
			const block = this.#blocks[i];
			if (block.kind !== 'work' || block.workedMs !== null) continue;
			for (let j = block.steps.length - 1; j >= 0; j -= 1) {
				const step = block.steps[j];
				if (step.kind === 'tool' && step.tool.running) return step.tool.name;
			}
		}
		return null;
	}

	liveActivity(): LiveActivity {
		const running = this.runningTool();
		if (running) return { kind: 'tool', name: running };
		for (let i = this.#blocks.length - 1; i >= 0; i -= 1) {
			const block = this.#blocks[i];
			if (block.kind === 'work') {
				if (block.workedMs !== null) continue;
				const last = block.steps.at(-1);
				if (!last || last.kind === 'thinking') {
					if (last?.kind === 'thinking' && last.durationMs !== null) {
						return { kind: 'writing' };
					}
					return { kind: 'thinking' };
				}
				return { kind: 'preparing' };
			}
			if (block.kind === 'text') {
				if (block.text.trim() === '') continue;
				return { kind: 'writing' };
			}
			return { kind: 'thinking' };
		}
		return { kind: 'thinking' };
	}

	private turnWorkIndex(): number | null {
		for (let i = this.#blocks.length - 1; i >= 0; i -= 1) {
			const block = this.#blocks[i];
			if (block.kind === 'text' || block.kind === 'user') return null;
			return i;
		}
		return null;
	}

	private latestWorkInTurn(): number | null {
		for (let i = this.#blocks.length - 1; i >= 0; i -= 1) {
			const block = this.#blocks[i];
			if (block.kind === 'text') continue;
			if (block.kind === 'user') return null;
			return i;
		}
		return null;
	}

	private reopenWork(): number | null {
		const idx = this.turnWorkIndex();
		if (idx === null) return null;
		const block = this.#blocks[idx];
		if (block.kind === 'work') {
			block.workedMs = null;
		}
		return idx;
	}

	private freezeOpenThinking() {
		const idx = this.openWorkIndex();
		if (idx === null) return;
		const block = this.#blocks[idx];
		if (block.kind === 'work') freezeThinkingSteps(block.steps);
	}

	private openWorkIndex(): number | null {
		for (let i = this.#blocks.length - 1; i >= 0; i -= 1) {
			const block = this.#blocks[i];
			if (block.kind === 'text' || block.kind === 'user') return null;
			if (block.kind === 'work' && block.workedMs === null) return i;
			return null;
		}
		return null;
	}
}

function thinkingStep(text: string): WorkStep {
	return {
		kind: 'thinking',
		text,
		expanded: false,
		startedAt: Date.now(),
		durationMs: null
	};
}

function openWork(steps: WorkStep[]): TranscriptBlock {
	return {
		kind: 'work',
		steps,
		expanded: false,
		startedAt: Date.now(),
		workedMs: null
	};
}

function freezeThinkingSteps(steps: WorkStep[]) {
	const last = steps.at(-1);
	if (last?.kind === 'thinking' && last.durationMs === null) {
		last.durationMs = Date.now() - last.startedAt;
	}
}

export function firstUserTitle(blocks: TranscriptBlock[], limit = 28): string | null {
	const user = blocks.find((block): block is Extract<TranscriptBlock, { kind: 'user' }> => {
		return block.kind === 'user';
	});
	if (!user) return null;
	const line = user.text.trim().split('\n')[0] ?? '';
	if (line.length === 0) return null;
	return line.length > limit ? `${[...line].slice(0, limit).join('')}…` : line;
}

export function compactDuration(durationMs: number): string {
	const secs = Math.floor(durationMs / 1000);
	if (secs < 60) return `${Math.max(1, secs)}s`;
	const mins = Math.floor(secs / 60);
	const rem = secs % 60;
	return rem === 0 ? `${mins}m` : `${mins}m ${rem}s`;
}

export function workedForLabel(durationMs: number): string {
	return `Worked for ${compactDuration(durationMs)}`;
}

export function thoughtLabel(
	durationMs: number | null,
	startedAt: number,
	live: boolean
): string {
	if (live) return 'Thinking';
	const elapsed = durationMs ?? Date.now() - startedAt;
	return `Thought ${compactDuration(elapsed)}`;
}

export function liveActivityLabel(activity: LiveActivity): string {
	switch (activity.kind) {
		case 'thinking':
			return 'Thinking';
		case 'preparing':
			return 'Preparing next moves';
		case 'writing':
			return 'Writing';
		case 'tool':
			return toolActivityLabel(activity.name).replace(/…$/, '');
	}
}

export function collapsedLabel(block: TranscriptBlock, thinkingLive: boolean): string {
	if (block.kind !== 'work') return '';
	if (block.workedMs !== null) return workedForLabel(block.workedMs);
	return liveWorkLabel(block.steps, thinkingLive);
}

function liveWorkLabel(steps: WorkStep[], thinkingLive: boolean): string {
	const tools = steps.filter((step): step is Extract<WorkStep, { kind: 'tool' }> => step.kind === 'tool');
	const current = [...tools].reverse().find((item) => item.tool.running);
	if (current) return toolActivityLabel(current.tool.name);
	if (thinkingLive || tools.length === 0) {
		const hasThinking = steps.some(
			(step) => step.kind === 'thinking' && step.text.trim().length > 0
		);
		return hasThinking && !thinkingLive ? 'Thought' : 'Thinking';
	}
	if (tools.length === 1) return toolDoneLabel(tools[0].tool.name);
	return `Used ${tools.length} tools`;
}

export type WorkHeaderVisual = {
	icon: string;
	tone: ToolTone;
};

export function workHeaderVisual(steps: WorkStep[]): WorkHeaderVisual | null {
	const tools = steps.filter((step): step is Extract<WorkStep, { kind: 'tool' }> => step.kind === 'tool');
	const current = [...tools].reverse().find((item) => item.tool.running);
	if (current) return toolVisual(current.tool.name);
	if (tools.length === 0) return null;
	if (tools.length === 1) return toolVisual(tools[0].tool.name);
	return { icon: '/icons/wrench.svg', tone: 'muted' };
}

export function heartbeatStatus(
	state: string,
	transcript: Transcript,
	activity: LiveActivity
): string | null {
	switch (state) {
		case 'preparing_context':
			return 'Gathering context';
		case 'running': {
			if (transcript.runningTool()) return null;
			if (activity.kind === 'writing' || activity.kind === 'tool') return null;
			if (activity.kind === 'thinking' && transcript.liveThinkingIndex() !== null) return null;
			return liveActivityLabel(activity);
		}
		default:
			return null;
	}
}

export function failedOffersSettings(message: string): boolean {
	const lower = message.toLowerCase();
	return (
		lower.includes('settings') ||
		lower.includes('api key') ||
		lower.includes('401') ||
		lower.includes('unauthorized') ||
		lower.includes('provider')
	);
}
