import { describe, expect, it } from 'vitest';
import { externalUrlToOpen } from './external-links';

const origin = 'http://localhost:5173';

describe('externalUrlToOpen', () => {
	it('sends http(s) links to the browser', () => {
		expect(externalUrlToOpen('https://opencode.ai/docs', origin)).toBe(
			'https://opencode.ai/docs'
		);
		expect(externalUrlToOpen('http://example.com/a?q=1', origin)).toBe(
			'http://example.com/a?q=1'
		);
	});

	it('keeps in-app routes in the WebView', () => {
		expect(externalUrlToOpen('/settings', origin)).toBeNull();
		expect(externalUrlToOpen('http://localhost:5173/settings', origin)).toBeNull();
		expect(externalUrlToOpen('#section', origin)).toBeNull();
	});

	it('opens mailto and tel, and ignores javascript', () => {
		expect(externalUrlToOpen('mailto:user@example.com', origin)).toBe('mailto:user@example.com');
		expect(externalUrlToOpen('tel:+15555550100', origin)).toBe('tel:+15555550100');
		expect(externalUrlToOpen('javascript:alert(1)', origin)).toBeNull();
		expect(externalUrlToOpen('file:///etc/passwd', origin)).toBeNull();
	});
});
