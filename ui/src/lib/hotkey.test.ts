import { describe, expect, it } from 'vitest';
import { modifierTokens, specFromKeyboardEvent } from './hotkey';

describe('specFromKeyboardEvent', () => {
	it('builds Option+Space as alt+Space', () => {
		expect(
			specFromKeyboardEvent({
				code: 'Space',
				altKey: true,
				ctrlKey: false,
				metaKey: false,
				shiftKey: false
			})
		).toBe('alt+Space');
	});

	it('ignores modifier-only presses and shortcuts without Option/Control/Command', () => {
		expect(
			specFromKeyboardEvent({
				code: 'AltLeft',
				altKey: true,
				ctrlKey: false,
				metaKey: false,
				shiftKey: false
			})
		).toBeNull();
		expect(
			specFromKeyboardEvent({
				code: 'Space',
				altKey: false,
				ctrlKey: false,
				metaKey: false,
				shiftKey: true
			})
		).toBeNull();
	});

	it('includes Shift with Command', () => {
		expect(
			specFromKeyboardEvent({
				code: 'KeyK',
				altKey: false,
				ctrlKey: false,
				metaKey: true,
				shiftKey: true
			})
		).toBe('shift+super+KeyK');
	});
});

describe('modifierTokens', () => {
	it('follows Control, Option, Shift, Command order', () => {
		expect(
			modifierTokens({
				altKey: true,
				ctrlKey: true,
				metaKey: true,
				shiftKey: true
			})
		).toEqual(['Control', 'Option', 'Shift', 'Command']);
	});
});
