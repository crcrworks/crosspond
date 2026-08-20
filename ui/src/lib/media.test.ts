import { describe, expect, it } from 'vitest';
import {
	attachmentNames,
	clipboardFiles,
	composerCanSend,
	fileNameOnly,
	isClipboardPathText,
	isPastedImage,
	pastedImages
} from './media';
import { displayPrompt } from './mentions';

describe('pasted images', () => {
	it('accepts common bitmap types and ignores video', () => {
		const png = new File([new Uint8Array([1])], 'shot.png', { type: 'image/png' });
		const video = new File([new Uint8Array([1])], 'clip.mov', { type: 'video/quicktime' });
		const heic = new File([new Uint8Array([1])], 'photo.heic', { type: 'image/heic' });
		expect(isPastedImage(png)).toBe(true);
		expect(isPastedImage(video)).toBe(false);
		expect(isPastedImage(heic)).toBe(false);
		expect(pastedImages([png, video, heic])).toEqual([png]);
		expect(
			pastedImages(
				clipboardFiles({
					files: [] as unknown as FileList,
					items: [
						{ kind: 'file', type: video.type, getAsFile: () => video },
						{ kind: 'file', type: png.type, getAsFile: () => png }
					]
				} as unknown as DataTransfer)
			)
		).toEqual([png]);
	});

	it('rejects empty and oversized images', () => {
		const empty = new File([], 'empty.png', { type: 'image/png' });
		const huge = new File([new Uint8Array(10 * 1024 * 1024 + 1)], 'huge.png', {
			type: 'image/png'
		});
		expect(isPastedImage(empty)).toBe(false);
		expect(isPastedImage(huge)).toBe(false);
	});
});

describe('composerCanSend', () => {
	it('allows an empty prompt when an attachment is present', () => {
		expect(composerCanSend('', 0, 0)).toBe(false);
		expect(composerCanSend('   ', 0, 0)).toBe(false);
		expect(composerCanSend('', 0, 1)).toBe(true);
		expect(composerCanSend('', 1, 0)).toBe(true);
		expect(composerCanSend('hello', 0, 0)).toBe(true);
	});
});

describe('displayPrompt with attachments', () => {
	it('puts file names before mentions and text', () => {
		expect(
			displayPrompt([{ kind: 'screen' }], 'これは何', [{ name: 'photo.png' }])
		).toBe('photo.png @screen これは何');
		expect(displayPrompt([], '', [{ name: 'clip.mov' }])).toBe('clip.mov');
		expect(displayPrompt([], 'これは何', [{ name: '/Users/me/photo.png' }])).toBe(
			'photo.png これは何'
		);
	});
});

describe('attachmentNames', () => {
	it('drops blank names', () => {
		expect(
			attachmentNames([
				{ id: '1', name: 'photo.png', kind: 'image' },
				{ id: '2', name: '  ', kind: 'video' }
			])
		).toEqual(['photo.png']);
	});
});

describe('clipboard path text', () => {
	it('treats Finder paths as something the composer must not insert', () => {
		expect(isClipboardPathText('/Users/me/photo.png')).toBe(true);
		expect(isClipboardPathText('file:///Users/me/photo.png')).toBe(true);
		expect(isClipboardPathText('look at this')).toBe(false);
		expect(fileNameOnly('/Users/me/photo.png')).toBe('photo.png');
	});
});
