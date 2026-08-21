import { describe, expect, it } from 'vitest';
import {
	chipLabel,
	displayPrompt,
	filterCatalog,
	filterSlashSkills,
	mentionTrigger,
	mergeMentions,
	skillTrigger,
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

describe('skillTrigger', () => {
	it('detects / and fullwidth ／ at the caret', () => {
		expect(skillTrigger('hello /pdf', 10)).toEqual({ start: 6, query: 'pdf' });
		const fullwidth = '見て ／lab';
		expect(skillTrigger(fullwidth, fullwidth.length)).toEqual({ start: 3, query: 'lab' });
		expect(skillTrigger('no skill', 8)).toBeNull();
		expect(skillTrigger('https://example.com/x', 21)).toBeNull();
	});
});

describe('filterCatalog', () => {
	it('matches token prefixes and Japanese descriptions', () => {
		expect(filterCatalog('sc').map((item) => item.kind)).toEqual(['screen']);
		expect(filterCatalog('画面').map((item) => item.kind)).toEqual(['screen', 'computer']);
		expect(filterCatalog('知識').map((item) => item.kind)).toEqual(['vault_query']);
		expect(filterCatalog('操作').map((item) => item.kind)).toEqual(['computer', 'browser']);
		expect(filterCatalog('').length).toBeGreaterThan(3);
	});

	it('lets query attach without a note picker', () => {
		const query = filterCatalog('query')[0];
		expect(query?.kind).toBe('vault_query');
		expect(query?.token).toBe('vault-query');
		expect(query?.needsPicker).toBe(false);
		expect(filterCatalog('app')[0]?.needsPicker).toBe(true);
	});

	it('groups vault mentions under vault and aliases', () => {
		expect(filterCatalog('vault').map((item) => item.kind)).toEqual([
			'vault_query',
			'vault_save',
			'vault_later',
			'vault_procedure'
		]);
		expect(filterCatalog('save').map((item) => item.kind)).toEqual(['vault_save']);
		expect(filterCatalog('later').map((item) => item.kind)).toEqual(['vault_later']);
		expect(filterCatalog('procedure').map((item) => item.kind)).toEqual(['vault_procedure']);
		expect(filterCatalog('手順').map((item) => item.kind)).toEqual(['vault_procedure']);
		expect(filterCatalog('computer').map((item) => item.kind)).toEqual(['computer']);
		expect(filterCatalog('browser').map((item) => item.kind)).toEqual(['browser']);
		expect(filterCatalog('borser').map((item) => item.kind)).toEqual(['browser']);
		expect(filterCatalog('chrome').map((item) => item.kind)).toEqual(['browser']);
		expect(filterCatalog('ブラ').map((item) => item.kind)).toEqual(['browser']);
		expect(filterCatalog('browser')[0]?.description).toBe('ブラウザを操作');
	});

	it('renames web search to @search and keeps @web as an alias', () => {
		const search = filterCatalog('search')[0];
		expect(search?.kind).toBe('search');
		expect(search?.token).toBe('search');
		expect(filterCatalog('web').map((item) => item.kind)).toEqual(['search']);
	});
});

describe('filterSlashSkills', () => {
	const skills = [
		{ name: 'pdf-processing', description: 'Extract text from PDF files', origin: 'local' as const },
		{ name: 'lab-notes', description: 'Global helper for lab PDFs', origin: 'global' as const }
	];

	it('filters by name and description', () => {
		expect(filterSlashSkills(skills, '').map((item) => item.name)).toEqual([
			'pdf-processing',
			'lab-notes'
		]);
		expect(filterSlashSkills(skills, 'pdf').map((item) => item.name)).toEqual([
			'pdf-processing',
			'lab-notes'
		]);
		expect(filterSlashSkills(skills, 'lab').map((item) => item.name)).toEqual(['lab-notes']);
	});
});

describe('takeInlineMentions', () => {
	it('strips known tokens and keeps the instruction', () => {
		const taken = takeInlineMentions('@query @screen VPN 調べて');
		expect(taken.prompt).toBe('VPN 調べて');
		expect(taken.mentions.map((item) => item.kind)).toEqual(['vault_query', 'screen']);
	});

	it('accepts hyphenated vault tokens and computer', () => {
		const taken = takeInlineMentions('@vault-query @computer 進めて');
		expect(taken.prompt).toBe('進めて');
		expect(taken.mentions.map((item) => item.kind)).toEqual(['vault_query', 'computer']);
	});

	it('accepts @vault-procedure and the procedure alias', () => {
		expect(
			takeInlineMentions('@vault-procedure 経費精算して').mentions.map((item) => item.kind)
		).toEqual(['vault_procedure']);
		expect(takeInlineMentions('@procedure').mentions.map((item) => item.kind)).toEqual([
			'vault_procedure'
		]);
	});

	it('accepts @search and the @web alias', () => {
		expect(takeInlineMentions('@search 調べて').mentions.map((item) => item.kind)).toEqual([
			'search'
		]);
		expect(takeInlineMentions('@web 調べて').mentions.map((item) => item.kind)).toEqual([
			'search'
		]);
	});

	it('accepts browser and the borser alias', () => {
		const taken = takeInlineMentions('@browser @borser Continue を押して');
		expect(taken.prompt).toBe('Continue を押して');
		expect(taken.mentions.map((item) => item.kind)).toEqual(['browser', 'browser']);
	});

	it('strips slash skill tokens', () => {
		const taken = takeInlineMentions('/pdf-processing この PDF まとめて');
		expect(taken.prompt).toBe('この PDF まとめて');
		expect(taken.mentions).toEqual([{ kind: 'skill', name: 'pdf-processing' }]);
		expect(takeInlineMentions('／pdf-processing 進めて').mentions).toEqual([
			{ kind: 'skill', name: 'pdf-processing' }
		]);
		expect(takeInlineMentions('see https://example.com/pdf-processing').mentions).toEqual([]);
	});
});

describe('displayPrompt', () => {
	it('joins chips and leftover text with hyphenated vault tokens', () => {
		expect(
			displayPrompt([{ kind: 'screen' }, { kind: 'vault_query' }], '進めて')
		).toBe('@screen @vault-query 進めて');
		expect(displayPrompt([{ kind: 'computer' }], '進めて')).toBe('@computer 進めて');
		expect(displayPrompt([{ kind: 'search' }], '調べて')).toBe('@search 調べて');
		expect(displayPrompt([{ kind: 'browser' }], 'クリックして')).toBe('@browser クリックして');
		expect(displayPrompt([{ kind: 'skill', name: 'pdf-processing' }], 'まとめて')).toBe(
			'/pdf-processing まとめて'
		);
		expect(displayPrompt([{ kind: 'vault_procedure' }], '経費精算して')).toBe(
			'@vault-procedure 経費精算して'
		);
	});
});

describe('mergeMentions', () => {
	it('dedupes singleton kinds', () => {
		const merged = mergeMentions(
			[{ kind: 'vault_query' }],
			[{ kind: 'vault_query' }, { kind: 'vault_save' }]
		);
		expect(merged.map((item) => item.kind)).toEqual(['vault_query', 'vault_save']);
	});

	it('dedupes slash skills by name', () => {
		const merged = mergeMentions(
			[{ kind: 'skill', name: 'pdf-processing' }],
			[
				{ kind: 'skill', name: 'pdf-processing' },
				{ kind: 'skill', name: 'lab-notes' }
			]
		);
		expect(merged).toEqual([
			{ kind: 'skill', name: 'pdf-processing' },
			{ kind: 'skill', name: 'lab-notes' }
		]);
	});
});

describe('chipLabel', () => {
	it('labels vault query without a note title', () => {
		expect(chipLabel({ kind: 'vault_query' })).toBe('Vault query');
		expect(chipLabel({ kind: 'vault_procedure' })).toBe('Vault procedure');
		expect(chipLabel({ kind: 'screen' })).toBe('Screen');
		expect(chipLabel({ kind: 'computer' })).toBe('Computer');
		expect(chipLabel({ kind: 'search' })).toBe('Search');
		expect(chipLabel({ kind: 'browser' })).toBe('Browser');
		expect(chipLabel({ kind: 'skill', name: 'pdf-processing' })).toBe('/pdf-processing');
	});
});
