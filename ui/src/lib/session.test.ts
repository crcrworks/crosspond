import { describe, expect, it } from 'vitest';
import { LauncherSession } from './session.svelte';

describe('LauncherSession completion', () => {
	it('keeps the summary in the transcript without needing Changed lines', () => {
		const session = new LauncherSession();
		session.beginTask('task-1', 'conv-1', 'Press Continue');
		session.applyEvent({
			type: 'task_completed',
			task_id: 'task-1',
			summary: 'Clicked Continue in Safari.',
			receipt: {
				task_id: 'task-1',
				summary: 'Clicked Continue in Safari.',
				actions: ['Clicked Continue'],
				artifacts: []
			}
		});
		const text = session.transcript
			.blocks()
			.filter((block) => block.kind === 'text')
			.map((block) => (block.kind === 'text' ? block.text : ''))
			.join('\n');
		expect(text).toContain('Clicked Continue in Safari.');
		expect(session.receipt?.actions).toEqual(['Clicked Continue']);
	});

	it('restoreConversation shows the past transcript as a follow-up chat', () => {
		const session = new LauncherSession();
		session.restoreConversation({
			id: 'conv-1',
			status: 'completed',
			transcript: [
				{ kind: 'user', text: 'Press Continue' },
				{ kind: 'text', text: 'Clicked Continue in Safari.' }
			],
			receipt: {
				task_id: 'task-1',
				summary: 'Clicked Continue in Safari.',
				actions: ['Clicked Continue'],
				artifacts: []
			},
			artifact_names: []
		});
		expect(session.currentConversation).toBe('conv-1');
		expect(session.state).toBe('completed');
		expect(session.inConversation).toBe(true);
		expect(session.placeholder).toBe('Ask a follow-up...');
		expect(session.transcript.blocks()[0]).toMatchObject({ kind: 'user', text: 'Press Continue' });
		session.mentions = [{ kind: 'screen' }];
		session.beginTask('task-2', 'conv-1', 'and then stop');
		expect(session.mentions).toEqual([]);
		expect(session.transcript.blocks()).toHaveLength(3);
		expect(session.transcript.blocks()[2]).toMatchObject({
			kind: 'user',
			text: 'and then stop'
		});
	});
});
