import { describe, expect, it } from 'vitest';
import {
	chipLabel,
	displayPrompt,
	filterCatalog,
	mentionTrigger,
	mergeMentions,
	takeInlineMentions
} from './mentions';

describe('mentionTrigger', () => {
	it('detects @ and fullwidth ＠ at the caret', () => {
		expect(mentionTrigger('hello @sc', 9)).toEqual({ start: 6, query: 'sc' });
		const fullwidth = '見て ＠vault';
		expect(mentionTrigger(fullwidth, fullwidth.length)).toEqual({ start: 3, query: 'vault' });
		expect(mentionTrigger('no mention', 10)).toBeNull();
		expect(mentionTrigger('email@x.com', 11)).toBeNull();
	});
});

describe('filterCatalog', () => {
	it('matches token prefixes and Japanese descriptions', () => {
		expect(filterCatalog('sc').map((item) => item.kind)).toEqual(['screen']);
		expect(filterCatalog('画面').map((item) => item.kind)).toEqual(['screen']);
		expect(filterCatalog('').length).toBeGreaterThan(3);
	});
});

describe('takeInlineMentions', () => {
	it('strips known tokens and keeps the instruction', () => {
		const taken = takeInlineMentions('@screen @save このダイアログ進めて');
		expect(taken.prompt).toBe('このダイアログ進めて');
		expect(taken.mentions.map((item) => item.kind)).toEqual(['screen', 'save']);
	});
});

describe('displayPrompt', () => {
	it('joins chips and leftover text', () => {
		expect(
			displayPrompt(
				[
					{ kind: 'screen' },
					{ kind: 'vault', note_id: 'cp_1', title: 'Lab VPN' }
				],
				'進めて'
			)
		).toBe('@screen @vault Lab VPN 進めて');
	});
});

describe('mergeMentions', () => {
	it('dedupes singleton kinds', () => {
		const merged = mergeMentions([{ kind: 'screen' }], [{ kind: 'screen' }, { kind: 'save' }]);
		expect(merged.map((item) => item.kind)).toEqual(['screen', 'save']);
	});
});

describe('chipLabel', () => {
	it('uses titles for vault notes', () => {
		expect(chipLabel({ kind: 'vault', title: 'Lab VPN' })).toBe('Lab VPN');
		expect(chipLabel({ kind: 'screen' })).toBe('Screen');
	});
});
