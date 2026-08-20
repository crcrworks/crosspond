/** @vitest-environment jsdom */
import { describe, expect, it } from 'vitest';
import { renderMarkdown } from './markdown';

describe('renderMarkdown', () => {
	it('strips script tags from untrusted markdown', () => {
		const html = renderMarkdown('Hello <script>alert(1)</script> **world**');
		expect(html).toContain('world');
		expect(html.toLowerCase()).not.toContain('<script');
		expect(html).not.toContain('alert(1)');
	});

	it('drops images', () => {
		const html = renderMarkdown('![x](javascript:alert(1))');
		expect(html.toLowerCase()).not.toContain('<img');
	});
});
