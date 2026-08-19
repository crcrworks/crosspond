/** Streaming must not re-apply the default conversation height. */
export function shouldSyncLauncherSize(compact: boolean, appliedCompact: boolean): boolean {
	return compact || appliedCompact;
}

export function composerExtraHeight(scrollHeight: number, minHeight = 24, maxHeight = 160): number {
	const next = Math.min(scrollHeight, maxHeight);
	return Math.max(0, next - minHeight);
}

export const PICKER_ROW_HEIGHT = 22;
