import { describe, expect, it } from 'vitest';
import {
	CUSTOM_MODEL,
	effortLabel,
	isCustomModelOption,
	modelOptionValue,
	parseModelOption
} from './models';

describe('effortLabel', () => {
	it('keeps known effort names', () => {
		expect(effortLabel('high')).toBe('high');
		expect(effortLabel('xhigh')).toBe('xhigh');
		expect(effortLabel('weird')).toBe('medium');
	});

	it('keeps custom as a UI sentinel', () => {
		expect(CUSTOM_MODEL).toBe('__custom__');
		expect(isCustomModelOption(CUSTOM_MODEL)).toBe(true);
		expect(isCustomModelOption('gpt-5.6-luna')).toBe(false);
	});
});

describe('modelOptionValue', () => {
	it('round-trips source and model ids that contain slashes', () => {
		const value = modelOptionValue('local', 'qwen/qwen3.6-35b-a3b');
		expect(parseModelOption(value)).toEqual({
			source: 'local',
			model: 'qwen/qwen3.6-35b-a3b'
		});
	});
});
