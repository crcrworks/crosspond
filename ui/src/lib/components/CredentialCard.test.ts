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
				title: 'Enter login for lab.fileserver',
				credentialRef: 'lab.fileserver',
				saveOffered: false,
				...handlers
			}
		});
		expect(body).toContain('Needs login');
		expect(body).toContain('Enter login for lab.fileserver');
		expect(body).toContain('lab.fileserver');
		expect(body).toContain('Username');
		expect(body).toContain('Password');
		expect(body).toContain('Submit');
		expect(body).toContain('Cancel');
		expect(body).not.toContain('Save in Keychain');
		expect(body).toContain('disabled');
	});

	it('offers a Keychain switch defaulting to Off when save is allowed', () => {
		const { body } = render(CredentialCard, {
			props: {
				title: 'Enter login for lab.fileserver',
				credentialRef: 'lab.fileserver',
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
