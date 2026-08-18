import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import TranscriptView from './TranscriptView.svelte';
import type { TranscriptBlock } from '$lib/transcript';

const work: TranscriptBlock = {
	kind: 'work',
	expanded: false,
	startedAt: 0,
	workedMs: null,
	steps: [
		{
			kind: 'tool',
			tool: { name: 'read_file', summary: 'notes.md', running: false }
		},
		{
			kind: 'tool',
			tool: { name: 'ui_press', summary: '', running: false }
		}
	]
};

const handlers = {
	thinkingLiveIndex: null as number | null,
	ontoggle: () => {},
	ontogglestep: () => {}
};

describe('TranscriptView preparing', () => {
	it('hides preparing next moves until the work group is expanded', () => {
		const { body } = render(TranscriptView, {
			props: { blocks: [work], preparing: true, ...handlers }
		});
		expect(body).toContain('Used 2 tools');
		expect(body).not.toContain('Preparing next moves');
	});

	it('lists preparing next moves with the expanded tool rows', () => {
		const { body } = render(TranscriptView, {
			props: { blocks: [{ ...work, expanded: true }], preparing: true, ...handlers }
		});
		expect(body).toContain('Used 2 tools');
		expect(body).toContain('read_file');
		expect(body).toContain('ui_press');
		expect(body).toContain('Preparing next moves');
		const toggle = body.match(/<button[\s\S]*?<\/button>/);
		expect(toggle?.[0]).toContain('Used 2 tools');
		expect(toggle?.[0]).not.toContain('Preparing next moves');
	});

	it('does not show preparing on a sealed work group', () => {
		const { body } = render(TranscriptView, {
			props: { blocks: [{ ...work, workedMs: 2500 }], preparing: true, ...handlers }
		});
		expect(body).toContain('Worked for 2s');
		expect(body).not.toContain('Preparing next moves');
	});
});
