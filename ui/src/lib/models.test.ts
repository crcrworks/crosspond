import { describe, expect, it } from 'vitest';
import { CUSTOM_MODEL, effortLabel } from './models';

describe('effortLabel', () => {
	it('keeps known effort names', () => {
		expect(effortLabel('high')).toBe('high');
		expect(effortLabel('xhigh')).toBe('xhigh');
		expect(effortLabel('weird')).toBe('medium');
	});

	it('keeps custom as a UI sentinel', () => {
		expect(CUSTOM_MODEL).toBe('__custom__');
	});
});
