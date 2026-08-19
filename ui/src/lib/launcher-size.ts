/** Extra height for first-launch onboarding on top of the compact bar.
 * Keep in sync with `ONBOARDING_EXTRA` in `crates/crosspond-app/src/launcher.rs`. */
export const ONBOARDING_EXTRA_HEIGHT = 80;

/** Streaming must not re-apply the default conversation height. */
export function shouldSyncLauncherSize(compact: boolean, appliedCompact: boolean): boolean {
	return compact || appliedCompact;
}

export function composerExtraHeight(scrollHeight: number, minHeight = 24, maxHeight = 160): number {
	const next = Math.min(scrollHeight, maxHeight);
	return Math.max(0, next - minHeight);
}
