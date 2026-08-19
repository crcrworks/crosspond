<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import {
		loadSettings,
		openSystemSettings,
		pauseLauncherHotkey,
		resumeLauncherHotkey,
		saveConfig,
		saveSecret,
		setLauncherHotkey,
		testConnection
	} from '$lib/api';
	import Badge from '$lib/components/Badge.svelte';
	import Button from '$lib/components/Button.svelte';
	import HotkeyTokens from '$lib/components/HotkeyTokens.svelte';
	import { modifierTokens, specFromKeyboardEvent } from '$lib/hotkey';
	import type { AgentEvent, HotkeyView, SettingsView } from '$lib/types';
	import { onMount } from 'svelte';

	let settings = $state<SettingsView | null>(null);
	let baseUrl = $state('');
	let model = $state('');
	let vaultPath = $state('');
	let allowedHosts = $state('');
	let blockedHosts = $state('');
	let apiKey = $state('');
	let exaKey = $state('');
	let saveStatus = $state<string | null>(null);
	let testStatus = $state<{ ok: boolean; message: string } | null>(null);
	let recording = $state(false);
	let draftTokens = $state<string[]>([]);
	let hotkeyError = $state<string | null>(null);

	const launcherHotkey = $derived(
		settings?.launcher_hotkey ?? { spec: 'alt+Space', tokens: ['Option', 'Space'] }
	);

	onMount(() => {
		void (async () => {
			const loaded = await loadSettings();
			settings = loaded;
			baseUrl = loaded.base_url;
			model = loaded.model;
			vaultPath = loaded.vault_path;
			allowedHosts = loaded.browser_allowed_hosts.join('\n');
			blockedHosts = loaded.browser_blocked_hosts.join('\n');
		})();
		let unlisten: (() => void) | undefined;
		void listen<AgentEvent>('agent-event', (event) => {
			if (event.payload.type === 'connection_tested') {
				testStatus = { ok: event.payload.ok, message: event.payload.message };
			}
		}).then((fn) => {
			unlisten = fn;
		});
		return () => {
			unlisten?.();
			void resumeLauncherHotkey();
		};
	});

	async function persist() {
		await saveConfig(
			baseUrl,
			model,
			vaultPath,
			splitHostLines(allowedHosts),
			splitHostLines(blockedHosts)
		);
		await saveSecret('provider', apiKey);
		await saveSecret('exa', exaKey);
		if (apiKey.trim()) apiKey = '';
		if (exaKey.trim()) exaKey = '';
		settings = await loadSettings();
		vaultPath = settings.vault_path;
		allowedHosts = settings.browser_allowed_hosts.join('\n');
		blockedHosts = settings.browser_blocked_hosts.join('\n');
	}

	async function onSave() {
		try {
			await persist();
			saveStatus = 'Saved.';
			testStatus = null;
		} catch (error) {
			saveStatus = null;
			testStatus = { ok: false, message: String(error) };
		}
	}

	async function onTest() {
		try {
			await persist();
			saveStatus = 'Saved.';
			testStatus = { ok: true, message: 'Testing connection…' };
			await testConnection();
		} catch (error) {
			testStatus = { ok: false, message: String(error) };
		}
	}

	async function startRecording() {
		if (recording) return;
		hotkeyError = null;
		saveStatus = null;
		draftTokens = [];
		try {
			await pauseLauncherHotkey();
			recording = true;
		} catch (error) {
			hotkeyError = String(error);
		}
	}

	async function cancelRecording() {
		if (!recording) return;
		recording = false;
		draftTokens = [];
		try {
			await resumeLauncherHotkey();
		} catch (error) {
			hotkeyError = String(error);
		}
	}

	function applyHotkey(view: HotkeyView) {
		if (!settings) return;
		settings = { ...settings, launcher_hotkey: view };
		hotkeyError = null;
		saveStatus = 'Shortcut saved.';
	}

	async function onRecordKey(event: KeyboardEvent) {
		if (!recording) return;
		event.preventDefault();
		if (event.key === 'Escape') {
			void cancelRecording();
			return;
		}
		if (event.repeat) return;
		draftTokens = modifierTokens(event);
		const spec = specFromKeyboardEvent(event);
		if (!spec) return;
		recording = false;
		draftTokens = [];
		try {
			const view = await setLauncherHotkey(spec);
			applyHotkey(view);
		} catch (error) {
			hotkeyError = String(error);
			try {
				await resumeLauncherHotkey();
			} catch (resumeError) {
				hotkeyError = `${error}; ${resumeError}`;
			}
		}
	}

	function onWindowBlur() {
		void cancelRecording();
	}

	function splitHostLines(text: string): string[] {
		return text
			.split('\n')
			.map((line) => line.trim())
			.filter((line) => line.length > 0);
	}
</script>

<svelte:window onkeydown={onRecordKey} onblur={onWindowBlur} />

<div class="h-full overflow-y-auto px-6 py-4 text-[var(--text)]" style:background="var(--bg)">
	<div class="flex flex-col gap-4">
		<div class="pt-2 text-xs font-semibold uppercase tracking-[0.05em] text-[var(--muted)]">
			General
		</div>
		<div class="flex flex-col gap-1">
			<div class="text-sm text-[var(--muted)]">Launch Crosspond</div>
			<button
				type="button"
				class="hotkey-record"
				aria-pressed={recording}
				aria-label={recording
					? 'Press a new launch shortcut. Escape cancels.'
					: `Launch shortcut ${launcherHotkey.tokens.join(' + ')}. Click to change.`}
				onclick={startRecording}
			>
				{#if recording}
					{#if draftTokens.length > 0}
						<HotkeyTokens tokens={draftTokens} />
					{:else}
						<span class="text-sm text-[var(--muted)]">Press a shortcut</span>
					{/if}
				{:else}
					<HotkeyTokens tokens={launcherHotkey.tokens} />
				{/if}
			</button>
			<div class="text-sm text-[var(--muted)]">
				Click, then press the new shortcut. Escape cancels.
			</div>
			{#if hotkeyError}
				<div class="text-sm" style:color="var(--danger)">{hotkeyError}</div>
			{/if}
		</div>
		<div class="pt-2 text-xs font-semibold uppercase tracking-[0.05em] text-[var(--muted)]">
			Knowledge
		</div>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Vault path</span>
			<input
				bind:value={vaultPath}
				placeholder={settings?.default_vault_path ?? ''}
				class="rounded-md border px-2 py-1 font-mono text-sm"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			/>
		</label>
		<div class="text-sm text-[var(--muted)]">
			Obsidian-compatible folder. Empty uses {settings?.default_vault_path ?? '~/Documents/Crosspond'}.
			Created if it does not exist.
		</div>
		<div class="pt-2 text-xs font-semibold uppercase tracking-[0.05em] text-[var(--muted)]">AI</div>
		<div class="flex flex-col gap-1">
			<div class="text-sm text-[var(--muted)]">Provider</div>
			<div>OpenAI Compatible</div>
		</div>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Base URL</span>
			<input
				bind:value={baseUrl}
				class="rounded-md border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			/>
		</label>
		<div class="text-sm text-[var(--muted)]">Must include /v1, e.g. http://127.0.0.1:1234/v1</div>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Model</span>
			<input
				bind:value={model}
				class="rounded-md border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			/>
		</label>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">API Key</span>
			<input
				bind:value={apiKey}
				type="password"
				placeholder={settings?.provider_key_stored
					? '••••••••  stored in Keychain'
					: 'Required — stored in Keychain'}
				class="rounded-md border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			/>
		</label>
		<div class="pt-2 text-xs font-semibold uppercase tracking-[0.05em] text-[var(--muted)]">
			Search
		</div>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Exa API Key</span>
			<input
				bind:value={exaKey}
				type="password"
				placeholder={settings?.exa_key_stored
					? '••••••••  stored in Keychain'
					: 'Optional — for web_search'}
				class="rounded-md border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			/>
		</label>
		<div class="text-sm text-[var(--muted)]">
			Required for web_search. Free credits at https://dashboard.exa.ai/api-keys
		</div>
		<div class="pt-2 text-xs font-semibold uppercase tracking-[0.05em] text-[var(--muted)]">
			Browser
		</div>
		<div class="surface flex flex-col gap-2">
			<div class="flex flex-row items-center gap-2">
				<div class="text-sm">Chrome extension</div>
				<Badge
					label={settings?.browser_connected ? 'Connected' : 'Not connected'}
					tone={settings?.browser_connected ? 'green' : 'muted'}
				/>
			</div>
			<div class="text-sm text-[var(--muted)]">
				chrome://extensions → Developer mode → Load unpacked → the folder below. Chromium pages then
				use DOM snapshots instead of Accessibility.
			</div>
			<div class="font-mono text-sm break-all">{settings?.browser_extension_path ?? 'extension/chrome'}</div>
		</div>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Allowed sites</span>
			<textarea
				bind:value={allowedHosts}
				rows="4"
				placeholder="example.com"
				class="rounded-md border px-2 py-1 font-mono text-sm"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			></textarea>
		</label>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Blocked sites</span>
			<textarea
				bind:value={blockedHosts}
				rows="3"
				class="rounded-md border px-2 py-1 font-mono text-sm"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			></textarea>
		</label>
		<div class="text-sm text-[var(--muted)]">
			One host per line. A new site still needs Allow, even in Auto. Blocked hosts are refused. Page
			contents are not shown here.
		</div>
		<div class="pt-2 text-xs font-semibold uppercase tracking-[0.05em] text-[var(--muted)]">
			Permissions
		</div>
		<div class="text-sm text-[var(--muted)]">
			Chat works without these. Enable them when you want selected text, screenshots, or calendar
			reads.
		</div>
		{#if settings}
			{#each [
				['Accessibility', settings.permissions.accessibility, 'accessibility'],
				['Screen Recording', settings.permissions.screen_recording, 'screen_recording'],
				['Calendars', settings.permissions.calendars, 'calendars']
			] as row (row[2])}
				<div class="surface flex flex-col gap-2">
					<div class="flex flex-row items-center gap-2">
						<div class="text-sm">{row[0]}</div>
						<Badge
							label={row[1] ? 'Enabled' : 'Not enabled'}
							tone={row[1] ? 'green' : 'muted'}
						/>
					</div>
					<Button
						label="Open System Settings"
						onclick={() =>
							void openSystemSettings(row[2] as 'accessibility' | 'screen_recording' | 'calendars')}
					/>
				</div>
			{/each}
		{/if}
		<div class="flex flex-row gap-2">
			<Button label="Save" onclick={() => void onSave()} variant="primary" />
			<Button label="Test Connection" onclick={() => void onTest()} />
		</div>
		{#if saveStatus}
			<div class="text-sm text-[var(--muted)]">{saveStatus}</div>
		{/if}
		{#if testStatus}
			<div class="text-sm" style:color={testStatus.ok ? 'var(--ok)' : 'var(--danger)'}
				>{testStatus.message}</div
			>
		{/if}
	</div>
</div>

<style>
	.hotkey-record {
		display: flex;
		width: fit-content;
		min-height: 32px;
		align-items: center;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface);
		padding: 6px 10px;
		cursor: pointer;
		text-align: left;
	}

	.hotkey-record:hover,
	.hotkey-record[aria-pressed='true'] {
		border-color: var(--prompt-border-focus);
	}
</style>
