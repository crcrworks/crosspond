export type MentionKind =
	| 'vault_query'
	| 'vault_save'
	| 'vault_later'
	| 'vault_procedure'
	| 'screen'
	| 'computer'
	| 'browser'
	| 'app'
	| 'files'
	| 'calendar'
	| 'search'
	| 'skill';

export type Mention =
	| { kind: 'vault_query' }
	| { kind: 'vault_save' }
	| { kind: 'vault_later' }
	| { kind: 'vault_procedure' }
	| { kind: 'screen' }
	| { kind: 'computer' }
	| { kind: 'browser' }
	| { kind: 'app'; name: string }
	| { kind: 'files' }
	| { kind: 'calendar' }
	| { kind: 'search' }
	| { kind: 'skill'; name: string };

export type SlashSkill = {
	name: string;
	description: string;
	origin: 'local' | 'global';
};

export type MentionCatalogItem = {
	kind: MentionKind;
	token: string;
	label: string;
	description: string;
	needsPicker: boolean;
	aliases?: string[];
};

export const MENTION_CATALOG: MentionCatalogItem[] = [
	{
		kind: 'vault_query',
		token: 'vault-query',
		label: 'Vault query',
		description: '知識を探す',
		needsPicker: false,
		aliases: ['query']
	},
	{
		kind: 'vault_save',
		token: 'vault-save',
		label: 'Vault save',
		description: 'Vault に残す',
		needsPicker: false,
		aliases: ['save']
	},
	{
		kind: 'vault_later',
		token: 'vault-later',
		label: 'Vault later',
		description: 'あとで読む',
		needsPicker: false,
		aliases: ['later']
	},
	{
		kind: 'vault_procedure',
		token: 'vault-procedure',
		label: 'Vault procedure',
		description: '手順として残す',
		needsPicker: false,
		aliases: ['procedure']
	},
	{
		kind: 'screen',
		token: 'screen',
		label: 'Screen',
		description: '画面を見る',
		needsPicker: false
	},
	{
		kind: 'computer',
		token: 'computer',
		label: 'Computer',
		description: '画面を見て操作する',
		needsPicker: false
	},
	{
		kind: 'browser',
		token: 'browser',
		label: 'Browser',
		description: 'ブラウザを操作',
		needsPicker: false,
		aliases: ['chrome', 'borser']
	},
	{
		kind: 'app',
		token: 'app',
		label: 'App',
		description: '対象アプリ',
		needsPicker: true
	},
	{
		kind: 'files',
		token: 'files',
		label: 'Files',
		description: '選択中のファイル',
		needsPicker: false
	},
	{
		kind: 'calendar',
		token: 'calendar',
		label: 'Calendar',
		description: '予定を見る',
		needsPicker: false
	},
	{
		kind: 'search',
		token: 'search',
		label: 'Search',
		description: 'ウェブで調べる',
		needsPicker: false,
		aliases: ['web']
	}
];

const TOKEN_TO_KIND: Record<string, MentionKind> = {
	'vault-query': 'vault_query',
	query: 'vault_query',
	'vault-save': 'vault_save',
	save: 'vault_save',
	'vault-later': 'vault_later',
	later: 'vault_later',
	'vault-procedure': 'vault_procedure',
	procedure: 'vault_procedure',
	screen: 'screen',
	computer: 'computer',
	browser: 'browser',
	borser: 'browser',
	chrome: 'browser',
	app: 'app',
	files: 'files',
	calendar: 'calendar',
	search: 'search',
	web: 'search'
};

const TOKEN_RE =
	/(^|\s)[@＠](vault-query|vault-save|vault-later|vault-procedure|computer|browser|borser|chrome|screen|save|later|procedure|files|calendar|search|web|query|app)\b/gi;

export function mentionTrigger(
	text: string,
	cursor: number
): { start: number; query: string } | null {
	const before = text.slice(0, cursor);
	const match = /(?:^|\s)([@＠][^\s@＠]*)$/.exec(before);
	if (!match) return null;
	return { start: cursor - match[1].length, query: match[1].slice(1) };
}

export function skillTrigger(
	text: string,
	cursor: number
): { start: number; query: string } | null {
	const before = text.slice(0, cursor);
	const match = /(?:^|\s)([/／][^\s/／]*)$/.exec(before);
	if (!match) return null;
	return { start: cursor - match[1].length, query: match[1].slice(1) };
}

export function filterCatalog(query: string): MentionCatalogItem[] {
	const needle = query.trim().toLowerCase();
	if (!needle) return MENTION_CATALOG;
	return MENTION_CATALOG.filter((item) => catalogMatches(item, needle, query.trim()));
}

function catalogMatches(item: MentionCatalogItem, needle: string, raw: string): boolean {
	if (item.token.startsWith(needle) || item.token.includes(`-${needle}`)) return true;
	if (item.label.toLowerCase().startsWith(needle) || item.label.toLowerCase().includes(needle)) {
		return true;
	}
	if (item.description.includes(raw) || item.description.toLowerCase().includes(needle)) {
		return true;
	}
	return (item.aliases ?? []).some((alias) => alias.startsWith(needle));
}

export function filterSlashSkills(skills: SlashSkill[], query: string): SlashSkill[] {
	const needle = query.trim().toLowerCase();
	if (!needle) return skills;
	return skills.filter((skill) => {
		if (skill.name.toLowerCase().startsWith(needle) || skill.name.toLowerCase().includes(needle)) {
			return true;
		}
		const description = skill.description.toLowerCase();
		return description.includes(needle) || description.includes(query.trim().toLowerCase());
	});
}

export function chipLabel(mention: Mention): string {
	switch (mention.kind) {
		case 'app':
			return mention.name.trim() || 'App';
		case 'vault_query':
			return 'Vault query';
		case 'vault_save':
			return 'Vault save';
		case 'vault_later':
			return 'Vault later';
		case 'vault_procedure':
			return 'Vault procedure';
		case 'screen':
			return 'Screen';
		case 'computer':
			return 'Computer';
		case 'browser':
			return 'Browser';
		case 'files':
			return 'Files';
		case 'calendar':
			return 'Calendar';
		case 'search':
			return 'Search';
		case 'skill':
			return `/${mention.name.trim() || 'skill'}`;
	}
}

export function displayPrompt(mentions: Mention[], text: string): string {
	const tokens = mentions.map((mention) => {
		if (mention.kind === 'app' && mention.name.trim()) {
			return `@app ${mention.name.trim()}`;
		}
		if (mention.kind === 'skill' && mention.name.trim()) {
			return `/${mention.name.trim()}`;
		}
		const item = MENTION_CATALOG.find((entry) => entry.kind === mention.kind);
		return `@${item?.token ?? mention.kind}`;
	});
	const trimmed = text.trim();
	if (trimmed) tokens.push(trimmed);
	return tokens.join(' ');
}

const SKILL_RE = /(^|\s)[/／]([a-z0-9]+(?:-[a-z0-9]+)*)\b/g;

export function takeInlineMentions(text: string): { prompt: string; mentions: Mention[] } {
	const mentions: Mention[] = [];
	let prompt = text.replace(TOKEN_RE, (_full, lead: string, token: string) => {
		const kind = TOKEN_TO_KIND[token.toLowerCase()];
		if (kind) mentions.push(mentionFromKind(kind));
		return lead;
	});
	prompt = prompt.replace(SKILL_RE, (_full, lead: string, name: string) => {
		mentions.push({ kind: 'skill', name });
		return lead;
	});
	prompt = prompt.replace(/\s+/g, ' ').trim();
	return { prompt, mentions };
}

export function mergeMentions(existing: Mention[], extra: Mention[]): Mention[] {
	const out = [...existing];
	for (const mention of extra) {
		if (mention.kind === 'app') {
			const key = mention.name || 'app';
			const duplicate = out.some(
				(item) => item.kind === 'app' && (item.name || 'app') === key
			);
			if (!duplicate) out.push(mention);
			continue;
		}
		if (mention.kind === 'skill') {
			const key = mention.name.trim();
			const duplicate = out.some((item) => item.kind === 'skill' && item.name.trim() === key);
			if (!duplicate) out.push(mention);
			continue;
		}
		if (!out.some((item) => item.kind === mention.kind)) out.push(mention);
	}
	return out;
}

export function mentionFromKind(kind: MentionKind, extra?: { name?: string }): Mention {
	if (kind === 'app') {
		return { kind: 'app', name: extra?.name ?? '' };
	}
	if (kind === 'skill') {
		return { kind: 'skill', name: extra?.name ?? '' };
	}
	return { kind };
}

export const MENTION_MENU_HEIGHT = 240;
export const MENTION_CHIP_ROW = 28;
