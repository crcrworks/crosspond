import { describe, expect, it } from 'vitest';
import { updateNoticeState } from './updater';

describe('updateNoticeState', () => {
	it('hides when nothing is available', () => {
		expect(
			updateNoticeState({ available: false, dismissed: false, installing: false })
		).toBe('hidden');
	});

	it('shows available until dismissed', () => {
		expect(
			updateNoticeState({ available: true, dismissed: false, installing: false })
		).toBe('available');
		expect(
			updateNoticeState({ available: true, dismissed: true, installing: false })
		).toBe('hidden');
	});

	it('shows installing even if dismissed', () => {
		expect(
			updateNoticeState({ available: true, dismissed: true, installing: true })
		).toBe('installing');
	});
});
