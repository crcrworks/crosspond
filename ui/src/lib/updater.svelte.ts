import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { updateNoticeState, type UpdateNotice } from './updater';

export class AppUpdater {
	available = $state(false);
	installing = $state(false);
	dismissed = $state(false);
	#pending: Update | null = null;

	get notice(): UpdateNotice {
		return updateNoticeState({
			available: this.available,
			dismissed: this.dismissed,
			installing: this.installing
		});
	}

	onLauncherShown = async (onboarding: boolean) => {
		if (onboarding || this.dismissed || this.installing) return;
		try {
			const update = await check();
			if (update) {
				this.#pending = update;
				this.available = true;
			}
		} catch {
			this.available = false;
			this.#pending = null;
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
