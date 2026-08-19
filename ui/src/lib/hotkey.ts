const MODIFIER_CODES = new Set([
	'ShiftLeft',
	'ShiftRight',
	'ControlLeft',
	'ControlRight',
	'AltLeft',
	'AltRight',
	'MetaLeft',
	'MetaRight'
]);

export type ModifierState = {
	altKey: boolean;
	ctrlKey: boolean;
	metaKey: boolean;
	shiftKey: boolean;
};

/** macOS HIG order: Control, Option, Shift, Command. */
export function modifierTokens(event: ModifierState): string[] {
	const tokens: string[] = [];
	if (event.ctrlKey) tokens.push('Control');
	if (event.altKey) tokens.push('Option');
	if (event.shiftKey) tokens.push('Shift');
	if (event.metaKey) tokens.push('Command');
	return tokens;
}

export function specFromKeyboardEvent(
	event: ModifierState & { code: string }
): string | null {
	if (MODIFIER_CODES.has(event.code)) return null;
	if (!event.altKey && !event.ctrlKey && !event.metaKey) return null;
	const parts: string[] = [];
	if (event.shiftKey) parts.push('shift');
	if (event.ctrlKey) parts.push('control');
	if (event.altKey) parts.push('alt');
	if (event.metaKey) parts.push('super');
	parts.push(event.code);
	return parts.join('+');
}
