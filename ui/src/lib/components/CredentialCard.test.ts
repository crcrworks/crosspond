import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import CredentialCard from './CredentialCard.svelte';

const handlers = {
	onfill: () => {},
	oncancel: () => {}
};

describe('CredentialCard', () => {
	it('shows login copy and the credential pointer, not a save switch by default', () => {
		const { body } = render(CredentialCard, {
			props: {
				title: 'Sign in to GitHub',
				credentialRef: 'vault:github',
				saveOffered: false,
				...handlers
			}
		});
		expect(body).toContain('Needs login');
		expect(body).toContain('Sign in to GitHub');
		expect(body).toContain('vault:github');
		expect(body).toContain('Username');
		expect(body).toContain('Password');
		expect(body).toContain('Fill');
		expect(body).toContain('Cancel');
		expect(body).not.toContain('Save in Keychain');
		expect(body).toContain('disabled');
	});

	it('offers a Keychain switch defaulting to Off when save is allowed', () => {
		const { body } = render(CredentialCard, {
			props: {
				title: 'Sign in to GitHub',
				credentialRef: 'vault:github',
				saveOffered: true,
				...handlers
			}
		});
		expect(body).toContain('Save in Keychain');
		expect(body).toContain('role="switch"');
		expect(body).toContain('Off');
		expect(body).not.toContain('>On<');
	});
});
