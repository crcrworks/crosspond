import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import PromptBar from './PromptBar.svelte';
import type { ComputerApproval } from '$lib/types';
import type { MediaAttachment } from '$lib/media';
import type { Mention } from '$lib/mentions';

const noop = () => {};

function props(
	overrides: {
		variant?: 'seamless' | 'docked';
		value?: string;
		mentions?: Mention[];
		attachments?: MediaAttachment[];
		approval?: ComputerApproval;
	} = {}
) {
	return {
		variant: overrides.variant ?? ('docked' as const),
		value: overrides.value ?? '',
		menuOpen: false,
		pickerOpen: false,
		mentionOpen: false,
		mentions: overrides.mentions ?? [],
		attachments: overrides.attachments ?? [],
		placeholder: 'Ask or do anything...',
		approval: overrides.approval ?? ('manual' as const),
		busy: false,
		canSubmit: true,
		onsubmit: noop,
		oncancel: noop,
		onapproval: noop,
		ongrow: noop,
		oncompositionstart: noop,
		oncompositionend: noop,
		onlistapps: async () => []
	};
}

function sendButton(body: string): string | undefined {
	return body.match(/<button[^>]*aria-label="Send"[^>]*>/)?.[0];
}

describe('PromptBar attachments', () => {
	it('shows file names only on chips', () => {
		const { body } = render(PromptBar, {
			props: props({
				attachments: [{ id: '1', name: '/Users/me/photo.png', kind: 'image' }]
			})
		});
		expect(body).toContain('prompt-chip');
		expect(body).toContain('photo.png');
		expect(body).toContain('Remove photo.png');
		expect(body).not.toContain('/Users');
		expect(body).not.toContain('\\x89PNG');
		expect(body).not.toContain('<img');
		expect(body).not.toContain('data:image');
	});

	it('keeps mention chips next to attachment names', () => {
		const { body } = render(PromptBar, {
			props: props({
				attachments: [{ id: '1', name: 'photo.png', kind: 'image' }],
				mentions: [{ kind: 'screen' }]
			})
		});
		expect(body).toContain('photo.png');
		expect(body).toContain('Screen');
		expect(body).toContain('Remove Screen');
	});

	it('enables send when the text is empty but an attachment is present', () => {
		const withFile = render(PromptBar, {
			props: props({
				attachments: [{ id: '1', name: 'clip.mov', kind: 'video' }]
			})
		});
		expect(sendButton(withFile.body)).toBeDefined();
		expect(sendButton(withFile.body)).not.toContain('disabled');

		const empty = render(PromptBar, { props: props() });
		expect(sendButton(empty.body)).toContain('disabled');
	});

	it('puts the paperclip at pickers-end when compact', () => {
		const { body } = render(PromptBar, { props: props({ variant: 'seamless' }) });
		const end = body.indexOf('prompt-pickers-end');
		const attach = body.indexOf('Attach image or video');
		expect(body).toContain('prompt-attach');
		expect(end).toBeGreaterThan(-1);
		expect(attach).toBeGreaterThan(end);
		expect(body).not.toContain('aria-label="Send"');
	});

	it('puts the paperclip left of Send when docked', () => {
		const { body } = render(PromptBar, { props: props() });
		const attach = body.indexOf('Attach image or video');
		const send = body.indexOf('aria-label="Send"');
		expect(body).toContain('prompt-attach');
		expect(attach).toBeGreaterThan(-1);
		expect(send).toBeGreaterThan(attach);
	});
});
