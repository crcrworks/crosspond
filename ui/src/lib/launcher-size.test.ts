import { describe, expect, it } from 'vitest';
import { composerExtraHeight, shouldSyncLauncherSize, ONBOARDING_EXTRA_HEIGHT } from './launcher-size';

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

describe('ONBOARDING_EXTRA_HEIGHT', () => {
	it('grows the compact bar enough for the ready copy and Open button', () => {
		expect(ONBOARDING_EXTRA_HEIGHT).toBe(80);
		expect(shouldSyncLauncherSize(true, true)).toBe(true);
	});
});
