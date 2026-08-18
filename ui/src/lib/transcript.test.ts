import { describe, expect, it } from 'vitest';
import {
	Transcript,
	collapsedLabel,
	heartbeatStatus,
	thoughtLabel,
	workHeaderIcon,
	workedForLabel
} from './transcript';
import { toolIconPath, toolRowLabel as row } from './tools';

function start(transcript: Transcript, name: string) {
	transcript.startTool(name, '');
}

describe('transcript', () => {
	it('consecutive tools and thinking share one group', () => {
		const transcript = new Transcript();
		transcript.pushReasoning('plan');
		start(transcript, 'get_accessibility_snapshot');
		transcript.finishTool('get_accessibility_snapshot');
		start(transcript, 'ui_press');
		transcript.finishTool('ui_press');
		transcript.pushText('Done.');
		start(transcript, 'read_file');
		expect(transcript.blocks()).toHaveLength(1);
		const block = transcript.blocks()[0];
		if (block.kind !== 'work') throw new Error('expected work');
		expect(block.steps).toHaveLength(5);
		expect(block.expanded).toBe(true);
		expect(block.workedMs).toBeNull();
		expect(block.steps[0]).toMatchObject({ kind: 'thinking', text: 'plan' });
		expect(block.steps[3]).toMatchObject({ kind: 'narration', text: 'Done.' });
		expect(block.steps[4]).toMatchObject({
			kind: 'tool',
			tool: { name: 'read_file', running: true }
		});
	});

	it('assistant text collapses work before the answer', () => {
		const transcript = new Transcript();
		transcript.pushReasoning('plan');
		start(transcript, 'read_file');
		transcript.finishTool('read_file');
		transcript.pushText('What was done.');
		expect(transcript.blocks()).toHaveLength(2);
		const work = transcript.blocks()[0];
		if (work.kind !== 'work') throw new Error('expected work');
		expect(work.expanded).toBe(false);
		expect(work.workedMs).not.toBeNull();
		start(transcript, 'run_command');
		expect(transcript.blocks()).toHaveLength(1);
		const reopened = transcript.blocks()[0];
		if (reopened.kind !== 'work') throw new Error('expected work');
		expect(reopened.expanded).toBe(true);
		expect(reopened.workedMs).toBeNull();
	});

	it('final text stays outside when work seals', () => {
		const transcript = new Transcript();
		transcript.pushReasoning('plan');
		start(transcript, 'read_file');
		transcript.finishTool('read_file');
		transcript.pushText('What was done.');
		transcript.finishRunningTools();
		expect(transcript.blocks()).toHaveLength(2);
		expect(transcript.blocks()[1]).toMatchObject({ kind: 'text', text: 'What was done.' });
	});

	it('intermediate text is absorbed when more tools run', () => {
		const transcript = new Transcript();
		transcript.pushText("I'll help.");
		start(transcript, 'list_directory');
		transcript.finishTool('list_directory');
		transcript.pushText('I found 5.');
		start(transcript, 'run_command');
		transcript.finishTool('run_command');
		transcript.pushText('What was done.');
		transcript.finishRunningTools();
		expect(transcript.blocks()).toHaveLength(2);
		const work = transcript.blocks()[0];
		if (work.kind !== 'work') throw new Error('expected work');
		expect(work.steps).toHaveLength(4);
		expect(work.steps[0]).toMatchObject({ kind: 'narration', text: "I'll help." });
		expect(work.steps[2]).toMatchObject({ kind: 'narration', text: 'I found 5.' });
	});

	it('user turn does not merge with assistant text', () => {
		const transcript = new Transcript();
		transcript.pushUser('hello');
		transcript.pushText('Hi there.');
		transcript.pushUser('again');
		transcript.pushText('Sure.');
		expect(transcript.blocks()).toHaveLength(4);
	});

	it('user turn closes work groups', () => {
		const transcript = new Transcript();
		transcript.pushUser('first');
		transcript.pushReasoning('plan');
		start(transcript, 'read_file');
		transcript.finishTool('read_file');
		transcript.pushText('Done.');
		transcript.pushUser('follow-up');
		transcript.pushReasoning('next');
		start(transcript, 'ui_press');
		expect(transcript.blocks()).toHaveLength(5);
	});

	it('follow-up keeps prior user and assistant', () => {
		const transcript = new Transcript();
		transcript.pushUser('summarize this');
		transcript.pushText('Here is a summary.');
		expect(transcript.hasAssistantTextSinceLastUser()).toBe(true);
		transcript.pushUser('make it shorter');
		expect(transcript.blocks()).toHaveLength(3);
		expect(transcript.hasAssistantTextSinceLastUser()).toBe(false);
	});

	it('collapsed work shows latest running then summary', () => {
		const transcript = new Transcript();
		start(transcript, 'get_accessibility_snapshot');
		expect(collapsedLabel(transcript.blocks()[0], false)).toBe('Looking at the screen…');
		transcript.finishTool('get_accessibility_snapshot');
		start(transcript, 'ui_press');
		expect(collapsedLabel(transcript.blocks()[0], false)).toBe('Pressing a control…');
		transcript.finishTool('ui_press');
		expect(collapsedLabel(transcript.blocks()[0], false)).toBe('Used 2 tools');
	});

	it('thinking and tools share one work group', () => {
		const transcript = new Transcript();
		transcript.pushReasoning('hmm');
		start(transcript, 'read_file');
		expect(transcript.blocks()).toHaveLength(1);
		transcript.toggle(0);
		const block = transcript.blocks()[0];
		if (block.kind !== 'work') throw new Error('expected work');
		expect(block.expanded).toBe(false);
	});

	it('finish running tools clears and seals', () => {
		const transcript = new Transcript();
		start(transcript, 'read_file');
		start(transcript, 'ui_press');
		expect(transcript.runningTool()).not.toBeNull();
		transcript.finishRunningTools();
		expect(transcript.runningTool()).toBeNull();
		expect(collapsedLabel(transcript.blocks()[0], false).startsWith('Worked for ')).toBe(true);
	});

	it('thinking between tools stays in the same group', () => {
		const transcript = new Transcript();
		transcript.pushReasoning('first');
		start(transcript, 'get_accessibility_snapshot');
		transcript.finishTool('get_accessibility_snapshot');
		transcript.pushReasoning(' more');
		start(transcript, 'ui_press');
		transcript.finishTool('ui_press');
		expect(transcript.blocks()).toHaveLength(1);
		expect(transcript.liveThinkingIndex()).toBeNull();
		transcript.pushReasoning('next');
		expect(transcript.liveThinkingIndex()).toBe(0);
	});

	it('whitespace-only text does not start a block', () => {
		const transcript = new Transcript();
		transcript.pushReasoning('plan');
		transcript.pushText('\n\n\n');
		start(transcript, 'ui_press');
		expect(transcript.blocks()).toHaveLength(1);
		const block = transcript.blocks()[0];
		if (block.kind !== 'work') throw new Error('expected work');
		expect(block.steps).toHaveLength(2);
	});

	it('worked for label formats seconds and minutes', () => {
		expect(workedForLabel(0)).toBe('Worked for 1s');
		expect(workedForLabel(1000)).toBe('Worked for 1s');
		expect(workedForLabel(12_000)).toBe('Worked for 12s');
		expect(workedForLabel(60_000)).toBe('Worked for 1m');
		expect(workedForLabel(179_000)).toBe('Worked for 2m 59s');
		expect(workedForLabel(180_000)).toBe('Worked for 3m');
	});

	it('thought label is thinking while live then duration', () => {
		const started = Date.now();
		expect(thoughtLabel(2000, started, true)).toBe('Thinking');
		expect(thoughtLabel(2000, started, false)).toBe('Thought 2s');
		expect(thoughtLabel(75_000, started, false)).toBe('Thought 1m 15s');
		expect(thoughtLabel(null, started, false).startsWith('Thought ')).toBe(true);
	});

	it('tool row label uses the tool name', () => {
		expect(row('knowledge_search', 'cursor origin')).toBe('knowledge_search  cursor origin');
		expect(row('ui_type', '')).toBe('ui_type');
	});

	it('tool icon matches known tools', () => {
		expect(toolIconPath('read_file')).toBe('/icons/file.svg');
		expect(toolIconPath('ui_click')).toBe('/icons/pointer.svg');
		expect(toolIconPath('unknown_tool')).toBe('/icons/wrench.svg');
	});

	it('live activity follows the agent phase', () => {
		const transcript = new Transcript();
		expect(transcript.liveActivity()).toEqual({ kind: 'thinking' });
		transcript.pushReasoning('plan');
		expect(transcript.liveActivity()).toEqual({ kind: 'thinking' });
		start(transcript, 'get_accessibility_snapshot');
		expect(transcript.liveActivity()).toEqual({
			kind: 'tool',
			name: 'get_accessibility_snapshot'
		});
		transcript.finishTool('get_accessibility_snapshot');
		expect(transcript.liveActivity()).toEqual({ kind: 'preparing' });
		transcript.pushText('Done.');
		expect(transcript.liveActivity()).toEqual({ kind: 'writing' });
	});

	it('header icon follows the latest running tool', () => {
		const transcript = new Transcript();
		start(transcript, 'get_accessibility_snapshot');
		const first = transcript.blocks()[0];
		if (first.kind !== 'work') throw new Error('expected work');
		expect(workHeaderIcon(first.steps)).toBe('/icons/monitor.svg');
		transcript.finishTool('get_accessibility_snapshot');
		start(transcript, 'ui_press');
		const second = transcript.blocks()[0];
		if (second.kind !== 'work') throw new Error('expected work');
		expect(workHeaderIcon(second.steps)).toBe('/icons/pointer.svg');
		transcript.finishTool('ui_press');
		expect(workHeaderIcon(second.steps)).toBe('/icons/wrench.svg');
	});

	it('toggle step expands nested thinking', () => {
		const transcript = new Transcript();
		transcript.pushReasoning('plan');
		start(transcript, 'read_file');
		transcript.toggleStep(0, 0);
		const block = transcript.blocks()[0];
		if (block.kind !== 'work' || block.steps[0].kind !== 'thinking') {
			throw new Error('expected thinking');
		}
		expect(block.steps[0].expanded).toBe(true);
	});

	it('heartbeat hides when idle done or writing', () => {
		const transcript = new Transcript();
		expect(heartbeatStatus('idle', transcript, { kind: 'thinking' })).toBeNull();
		expect(heartbeatStatus('completed', transcript, { kind: 'thinking' })).toBeNull();
		transcript.pushText('Done.');
		expect(heartbeatStatus('running', transcript, { kind: 'writing' })).toBeNull();
	});

	it('heartbeat shows when the screen would otherwise sit still', () => {
		const transcript = new Transcript();
		expect(heartbeatStatus('preparing_context', transcript, { kind: 'thinking' })).toBe(
			'Gathering context'
		);
		expect(heartbeatStatus('running', transcript, { kind: 'thinking' })).toBe('Thinking');
		start(transcript, 'read_file');
		expect(heartbeatStatus('running', transcript, { kind: 'tool', name: 'read_file' })).toBeNull();
		transcript.finishTool('read_file');
		expect(heartbeatStatus('running', transcript, { kind: 'preparing' })).toBe(
			'Preparing next moves'
		);
		transcript.pushReasoning('plan');
		expect(heartbeatStatus('running', transcript, { kind: 'thinking' })).toBeNull();
	});

	it('snapshot copies streamed text so later deltas do not mutate the view copy', () => {
		const transcript = new Transcript();
		transcript.pushText('Hel');
		const first = transcript.snapshot();
		transcript.pushText('lo');
		expect(first[0]).toMatchObject({ kind: 'text', text: 'Hel' });
		expect(transcript.snapshot()[0]).toMatchObject({ kind: 'text', text: 'Hello' });
		expect(first).not.toBe(transcript.blocks());
	});
});
