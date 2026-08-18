import DOMPurify from 'dompurify';
import { marked } from 'marked';

const renderer = new marked.Renderer();
renderer.image = () => '';
marked.use({ renderer, gfm: true });

export function renderMarkdown(source: string): string {
	const html = marked.parse(source, { async: false }) as string;
	return DOMPurify.sanitize(html, {
		FORBID_TAGS: ['img', 'script', 'iframe', 'object', 'embed']
	});
}
