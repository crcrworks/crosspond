export const MAX_ATTACHMENTS = 8;
export const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

export type MediaKind = 'image' | 'video';

export type MediaAttachment = {
	id: string;
	name: string;
	kind: MediaKind;
};

const PASTE_TYPES = new Set(['image/png', 'image/jpeg', 'image/jpg', 'image/gif', 'image/webp']);

export function isPastedImage(file: File): boolean {
	return PASTE_TYPES.has(file.type.toLowerCase()) && file.size > 0 && file.size <= MAX_IMAGE_BYTES;
}

export function pastedImages(list: FileList | File[] | null | undefined): File[] {
	if (!list) return [];
	return [...list].filter(isPastedImage).slice(0, MAX_ATTACHMENTS);
}

export function clipboardFiles(data: DataTransfer | null | undefined): File[] {
	if (!data) return [];
	if (data.files.length > 0) {
		return [...data.files];
	}
	const files: File[] = [];
	for (let i = 0; i < data.items.length; i += 1) {
		const item = data.items[i];
		if (!item || item.kind !== 'file') continue;
		const file = item.getAsFile();
		if (file) files.push(file);
	}
	return files;
}

export function isClipboardPathText(text: string): boolean {
	const first = text.trim().split(/\r?\n/, 1)[0] ?? '';
	return (
		first.startsWith('/') ||
		first.startsWith('~/') ||
		first.startsWith('file:') ||
		/^[A-Za-z]:[\\/]/.test(first)
	);
}

export function fileNameOnly(name: string): string {
	const trimmed = name.trim();
	const cut = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
	return cut === -1 ? trimmed : trimmed.slice(cut + 1);
}

export function composerCanSend(
	text: string,
	mentionCount: number,
	attachmentCount: number
): boolean {
	return text.trim().length > 0 || mentionCount > 0 || attachmentCount > 0;
}

export async function fileToBase64(file: File): Promise<string> {
	const buffer = await file.arrayBuffer();
	const bytes = new Uint8Array(buffer);
	let binary = '';
	for (const byte of bytes) {
		binary += String.fromCharCode(byte);
	}
	return btoa(binary);
}

export function attachmentNames(attachments: MediaAttachment[]): string[] {
	return attachments.map((item) => fileNameOnly(item.name)).filter((name) => name.length > 0);
}
