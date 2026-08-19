import { describe, expect, it } from 'vitest';
import { composerExtraHeight, PICKER_ROW_HEIGHT, shouldSyncLauncherSize } from './launcher-size';

describe('shouldSyncLauncherSize', () => {
	it('syncs when shrinking back to the compact bar', () => {
		expect(shouldSyncLauncherSize(true, false)).toBe(true);
		expect(shouldSyncLauncherSize(true, true)).toBe(true);
	});

	it('syncs on the first compact-to-conversation expand', () => {
		expect(shouldSyncLauncherSize(false, true)).toBe(true);
	});

	it('skips once the conversation window is already open', () => {
		expect(shouldSyncLauncherSize(false, false)).toBe(false);
	});
});

describe('composerExtraHeight', () => {
	it('is zero for a single-line field', () => {
		expect(composerExtraHeight(24)).toBe(0);
	});

	it('grows with wrapped lines and caps at the max field height', () => {
		expect(composerExtraHeight(64)).toBe(40);
		expect(composerExtraHeight(200)).toBe(136);
	});
});

describe('picker row', () => {
	it('reserves a compact row under the prompt', () => {
		expect(PICKER_ROW_HEIGHT).toBe(22);
	});
});
