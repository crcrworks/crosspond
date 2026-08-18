import { describe, expect, it } from 'vitest';
import { LauncherSession } from './session.svelte';

describe('LauncherSession completion', () => {
	it('keeps the summary in the transcript without needing Changed lines', () => {
		const session = new LauncherSession();
		session.beginTask('task-1', 'Press Continue');
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
});
