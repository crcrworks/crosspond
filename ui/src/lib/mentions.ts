export type MentionKind =
	| 'vault'
	| 'save'
	| 'later'
	| 'screen'
	| 'app'
	| 'files'
	| 'calendar'
	| 'web';

export type Mention =
	| { kind: 'vault'; note_id?: string; title?: string }
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
	needsQuery: boolean;
};

export type VaultMentionHit = {
	id: string;
	title: string;
	kind: string;
};

export const MENTION_CATALOG: MentionCatalogItem[] = [
	{
		kind: 'vault',
		token: 'vault',
		label: 'Vault',
		description: 'ノートを参照する',
		needsQuery: true
	},
	{
		kind: 'save',
		token: 'save',
		label: 'Save',
		description: 'Vault に残す',
		needsQuery: false
	},
	{
		kind: 'later',
		token: 'later',
		label: 'Later',
		description: 'あとで読む',
		needsQuery: false
	},
	{
		kind: 'screen',
		token: 'screen',
		label: 'Screen',
		description: '画面を見る',
		needsQuery: false
	},
	{
		kind: 'app',
		token: 'app',
		label: 'App',
		description: '対象アプリ',
		needsQuery: true
	},
	{
		kind: 'files',
		token: 'files',
		label: 'Files',
		description: '選択中のファイル',
		needsQuery: false
	},
	{
		kind: 'calendar',
		token: 'calendar',
		label: 'Calendar',
		description: '予定を見る',
		needsQuery: false
	},
	{
		kind: 'web',
		token: 'web',
		label: 'Web',
		description: 'ウェブで調べる',
		needsQuery: false
	}
];

const TOKEN_RE = /(^|\s)[@＠](screen|save|later|files|calendar|web|vault|app)\b/gi;

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
		case 'vault':
			return mention.title?.trim() || 'Vault';
		case 'app':
			return mention.name.trim() || 'App';
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
		if (mention.kind === 'vault' && mention.title?.trim()) {
			return `@vault ${mention.title.trim()}`;
		}
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
		if (mention.kind === 'vault' || mention.kind === 'app') {
			const key =
				mention.kind === 'vault'
					? mention.note_id || mention.title || 'vault'
					: mention.name || 'app';
			const duplicate = out.some((item) => {
				if (item.kind !== mention.kind) return false;
				if (item.kind === 'vault' && mention.kind === 'vault') {
					return (item.note_id || item.title || 'vault') === key;
				}
				if (item.kind === 'app' && mention.kind === 'app') {
					return (item.name || 'app') === key;
				}
				return false;
			});
			if (!duplicate) out.push(mention);
			continue;
		}
		if (!out.some((item) => item.kind === mention.kind)) out.push(mention);
	}
	return out;
}

export function mentionFromKind(kind: MentionKind, extra?: { name?: string; note_id?: string; title?: string }): Mention {
	if (kind === 'vault') {
		return { kind: 'vault', note_id: extra?.note_id, title: extra?.title };
	}
	if (kind === 'app') {
		return { kind: 'app', name: extra?.name ?? '' };
	}
	return { kind };
}

export const MENTION_MENU_HEIGHT = 240;
export const MENTION_CHIP_ROW = 28;
