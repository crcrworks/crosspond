export type MentionKind =
	| 'vault_query'
	| 'vault_save'
	| 'vault_later'
	| 'screen'
	| 'computer'
	| 'app'
	| 'files'
	| 'calendar'
	| 'web';

export type Mention =
	| { kind: 'vault_query' }
	| { kind: 'vault_save' }
	| { kind: 'vault_later' }
	| { kind: 'screen' }
	| { kind: 'computer' }
	| { kind: 'app'; name: string }
	| { kind: 'files' }
	| { kind: 'calendar' }
	| { kind: 'web' };

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
		kind: 'web',
		token: 'web',
		label: 'Web',
		description: 'ウェブで調べる',
		needsPicker: false
	}
];

const TOKEN_TO_KIND: Record<string, MentionKind> = {
	'vault-query': 'vault_query',
	query: 'vault_query',
	'vault-save': 'vault_save',
	save: 'vault_save',
	'vault-later': 'vault_later',
	later: 'vault_later',
	screen: 'screen',
	computer: 'computer',
	app: 'app',
	files: 'files',
	calendar: 'calendar',
	web: 'web'
};

const TOKEN_RE =
	/(^|\s)[@＠](vault-query|vault-save|vault-later|computer|screen|save|later|files|calendar|web|query|app)\b/gi;

export function mentionTrigger(
	text: string,
	cursor: number
): { start: number; query: string } | null {
	const before = text.slice(0, cursor);
	const match = /(?:^|\s)([@＠][^\s@＠]*)$/.exec(before);
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
		case 'screen':
			return 'Screen';
		case 'computer':
			return 'Computer';
		case 'files':
			return 'Files';
		case 'calendar':
			return 'Calendar';
		case 'web':
			return 'Web';
	}
}

export function displayPrompt(mentions: Mention[], text: string): string {
	const tokens = mentions.map((mention) => {
		if (mention.kind === 'app' && mention.name.trim()) {
			return `@app ${mention.name.trim()}`;
		}
		const item = MENTION_CATALOG.find((entry) => entry.kind === mention.kind);
		return `@${item?.token ?? mention.kind}`;
	});
	const trimmed = text.trim();
	if (trimmed) tokens.push(trimmed);
	return tokens.join(' ');
}

export function takeInlineMentions(text: string): { prompt: string; mentions: Mention[] } {
	const mentions: Mention[] = [];
	const prompt = text
		.replace(TOKEN_RE, (_full, lead: string, token: string) => {
			const kind = TOKEN_TO_KIND[token.toLowerCase()];
			if (kind) mentions.push(mentionFromKind(kind));
			return lead;
		})
		.replace(/\s+/g, ' ')
		.trim();
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
		if (!out.some((item) => item.kind === mention.kind)) out.push(mention);
	}
	return out;
}

export function mentionFromKind(kind: MentionKind, extra?: { name?: string }): Mention {
	if (kind === 'app') {
		return { kind: 'app', name: extra?.name ?? '' };
	}
	return { kind };
}

export const MENTION_MENU_HEIGHT = 240;
export const MENTION_CHIP_ROW = 28;
