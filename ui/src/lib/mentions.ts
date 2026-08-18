export type MentionKind =
	| 'query'
	| 'save'
	| 'later'
	| 'screen'
	| 'app'
	| 'files'
	| 'calendar'
	| 'web';

export type Mention =
	| { kind: 'query' }
	| { kind: 'save' }
	| { kind: 'later' }
	| { kind: 'screen' }
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
};

export const MENTION_CATALOG: MentionCatalogItem[] = [
	{
		kind: 'query',
		token: 'query',
		label: 'Query',
		description: '知識を探す',
		needsPicker: false
	},
	{
		kind: 'save',
		token: 'save',
		label: 'Save',
		description: 'Vault に残す',
		needsPicker: false
	},
	{
		kind: 'later',
		token: 'later',
		label: 'Later',
		description: 'あとで読む',
		needsPicker: false
	},
	{
		kind: 'screen',
		token: 'screen',
		label: 'Screen',
		description: '画面を見る',
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

const TOKEN_RE = /(^|\s)[@＠](screen|save|later|files|calendar|web|query|app)\b/gi;

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
	return MENTION_CATALOG.filter(
		(item) =>
			item.token.startsWith(needle) ||
			item.label.toLowerCase().startsWith(needle) ||
			item.description.includes(needle)
	);
}

export function chipLabel(mention: Mention): string {
	switch (mention.kind) {
		case 'app':
			return mention.name.trim() || 'App';
		case 'query':
			return 'Query';
		case 'save':
			return 'Save';
		case 'later':
			return 'Later';
		case 'screen':
			return 'Screen';
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
		return `@${mention.kind}`;
	});
	const trimmed = text.trim();
	if (trimmed) tokens.push(trimmed);
	return tokens.join(' ');
}

export function takeInlineMentions(text: string): { prompt: string; mentions: Mention[] } {
	const mentions: Mention[] = [];
	const prompt = text
		.replace(TOKEN_RE, (_full, lead: string, token: string) => {
			const kind = token.toLowerCase() as MentionKind;
			mentions.push(mentionFromKind(kind));
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
