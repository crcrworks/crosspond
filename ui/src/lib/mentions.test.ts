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
		const fullwidth = '見て ＠query';
		expect(mentionTrigger(fullwidth, fullwidth.length)).toEqual({ start: 3, query: 'query' });
		expect(mentionTrigger('no mention', 10)).toBeNull();
		expect(mentionTrigger('email@x.com', 11)).toBeNull();
	});
});

describe('filterCatalog', () => {
	it('matches token prefixes and Japanese descriptions', () => {
		expect(filterCatalog('sc').map((item) => item.kind)).toEqual(['screen']);
		expect(filterCatalog('画面').map((item) => item.kind)).toEqual(['screen']);
		expect(filterCatalog('知識').map((item) => item.kind)).toEqual(['query']);
		expect(filterCatalog('').length).toBeGreaterThan(3);
	});

	it('lets query attach without a note picker', () => {
		const query = filterCatalog('query')[0];
		expect(query?.kind).toBe('query');
		expect(query?.needsPicker).toBe(false);
		expect(filterCatalog('app')[0]?.needsPicker).toBe(true);
	});
});

describe('takeInlineMentions', () => {
	it('strips known tokens and keeps the instruction', () => {
		const taken = takeInlineMentions('@query @screen VPN 調べて');
		expect(taken.prompt).toBe('VPN 調べて');
		expect(taken.mentions.map((item) => item.kind)).toEqual(['query', 'screen']);
	});
});

describe('displayPrompt', () => {
	it('joins chips and leftover text', () => {
		expect(displayPrompt([{ kind: 'screen' }, { kind: 'query' }], '進めて')).toBe(
			'@screen @query 進めて'
		);
	});
});

describe('mergeMentions', () => {
	it('dedupes singleton kinds', () => {
		const merged = mergeMentions([{ kind: 'query' }], [{ kind: 'query' }, { kind: 'save' }]);
		expect(merged.map((item) => item.kind)).toEqual(['query', 'save']);
	});
});

describe('chipLabel', () => {
	it('labels query without a note title', () => {
		expect(chipLabel({ kind: 'query' })).toBe('Query');
		expect(chipLabel({ kind: 'screen' })).toBe('Screen');
	});
});
