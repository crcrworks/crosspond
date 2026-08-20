import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { dev } from '$app/environment';
import { shouldCheckForUpdates, updateNoticeState, type UpdateNotice } from './updater';

export class AppUpdater {
	available = $state(false);
	installing = $state(false);
	dismissed = $state(false);
	checking = $state(false);
	#pending: Update | null = null;

	get notice(): UpdateNotice {
		return updateNoticeState({
			available: this.available,
			dismissed: this.dismissed,
			installing: this.installing
		});
	}

	onLauncherShown = async (onboarding: boolean) => {
		if (!shouldCheckForUpdates(dev)) return;
		if (onboarding || this.dismissed || this.installing || this.checking) return;
		this.checking = true;
		try {
			const update = await check();
			if (update) {
				this.#pending = update;
				this.available = true;
			}
		} catch {
			this.available = false;
			this.#pending = null;
		} finally {
			this.checking = false;
		}
	};

	dismiss = () => {
		this.dismissed = true;
	};

	install = async () => {
		if (!this.#pending || this.installing) return;
		this.installing = true;
		try {
			await this.#pending.downloadAndInstall();
			await relaunch();
		} catch {
			this.installing = false;
		}
	};
}

export const appUpdater = new AppUpdater();
