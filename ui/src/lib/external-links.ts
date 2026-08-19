const BROWSER_PROTOCOLS = new Set(['http:', 'https:', 'mailto:', 'tel:']);

export function externalUrlToOpen(href: string | null, pageOrigin: string): string | null {
	if (!href || href.startsWith('#') || href.toLowerCase().startsWith('javascript:')) {
		return null;
	}
	let url: URL;
	try {
		url = new URL(href, pageOrigin);
	} catch {
		return null;
	}
	if (url.origin === pageOrigin) {
		return null;
	}
	if (!BROWSER_PROTOCOLS.has(url.protocol)) {
		return null;
	}
	return url.href;
}

export function externalUrlFromClick(event: Event, pageOrigin: string): string | null {
	if (event.defaultPrevented) {
		return null;
	}
	if (event instanceof MouseEvent && event.type === 'auxclick' && event.button !== 1) {
		return null;
	}
	const target = event.target;
	if (!(target instanceof Element)) {
		return null;
	}
	const anchor = target.closest('a');
	if (!(anchor instanceof HTMLAnchorElement)) {
		return null;
	}
	return externalUrlToOpen(anchor.getAttribute('href'), pageOrigin);
}
