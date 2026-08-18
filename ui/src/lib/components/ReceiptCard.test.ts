import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import ReceiptCard from './ReceiptCard.svelte';
import type { Receipt } from '$lib/types';

const receipt: Receipt = {
	task_id: 'task-1',
	summary: 'Clicked Continue in Safari.',
	actions: ['Clicked Continue', 'Opened a URL'],
	artifacts: []
};

describe('ReceiptCard', () => {
	it('omits Changed action lines after a task', () => {
		const { body } = render(ReceiptCard, {
			props: {
				receipt,
				onreveal: () => {}
			}
		});
		expect(body).not.toContain('Changed');
		expect(body).not.toContain('Clicked Continue');
		expect(body).not.toContain('Opened a URL');
		expect(body).not.toContain('Artifacts');
	});

	it('still lists artifacts with Show in Finder', () => {
		const { body } = render(ReceiptCard, {
			props: {
				receipt: { ...receipt, artifacts: ['notes.txt'] },
				names: ['notes.txt'],
				onreveal: () => {}
			}
		});
		expect(body).toContain('notes.txt');
		expect(body).toContain('Show in Finder');
		expect(body).not.toContain('Changed');
		expect(body).not.toContain('Clicked Continue');
	});
});
