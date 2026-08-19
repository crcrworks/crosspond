<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import {
		completeChatgptLogin,
		loadSettings,
		openSystemSettings,
		pauseLauncherHotkey,
		resumeLauncherHotkey,
		saveConfig,
		saveSecret,
		setLauncherHotkey,
		signOutChatgpt,
		startChatgptLogin,
		testConnection
	} from '$lib/api';
	import Badge from '$lib/components/Badge.svelte';
	import Button from '$lib/components/Button.svelte';
	import HotkeyTokens from '$lib/components/HotkeyTokens.svelte';
	import { modifierTokens, specFromKeyboardEvent } from '$lib/hotkey';
	import type { AgentEvent, HotkeyView, SettingsView } from '$lib/types';
	import { onMount } from 'svelte';

	let settings = $state<SettingsView | null>(null);
	let provider = $state<SettingsView['provider']>('openai_compatible');
	let baseUrl = $state('');
	let model = $state('');
	let vaultPath = $state('');
	let apiKey = $state('');
	let exaKey = $state('');
	let redirectUrl = $state('');
	let saveStatus = $state<string | null>(null);
	let testStatus = $state<{ ok: boolean; message: string } | null>(null);
	let loginStatus = $state<{ ok: boolean; message: string } | null>(null);
	let loginMode = $state<'idle' | 'browser' | 'manual'>('idle');
	let authorizeUrl = $state('');
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
			provider = loaded.provider;
			baseUrl = loaded.base_url;
			model = loaded.model;
			vaultPath = loaded.vault_path;
		})();
		let unlisten: (() => void) | undefined;
		void listen<AgentEvent>('agent-event', (event) => {
			if (event.payload.type === 'connection_tested') {
				testStatus = { ok: event.payload.ok, message: event.payload.message };
			}
		}).then((fn) => {
			unlisten = fn;
		});
		let unlistenLogin: (() => void) | undefined;
		void listen<{ ok: boolean; message: string }>('chatgpt-login', (event) => {
			loginStatus = event.payload;
			loginMode = 'idle';
			if (event.payload.ok) {
				void loadSettings().then((loaded) => {
					settings = loaded;
					provider = loaded.provider;
					model = loaded.model;
				});
			}
		}).then((fn) => {
			unlistenLogin = fn;
		});
		return () => {
			unlisten?.();
			unlistenLogin?.();
			void resumeLauncherHotkey();
		};
	});

	async function persist() {
		await saveConfig(baseUrl, model, vaultPath, provider);
		if (apiKey.trim()) {
			await saveSecret('provider', apiKey);
			apiKey = '';
		}
		if (exaKey.trim()) {
			await saveSecret('exa', exaKey);
			exaKey = '';
		}
		settings = await loadSettings();
		provider = settings.provider;
		model = settings.model;
		vaultPath = settings.vault_path;
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

	async function chooseProvider(next: SettingsView['provider']) {
		provider = next;
		loginStatus = null;
		loginMode = 'idle';
		if (next === 'chatgpt_codex' && model === 'gpt-4o-mini') {
			model = 'gpt-5.2';
		}
	}

	async function onChatgptLogin() {
		loginStatus = null;
		try {
			await persist();
			const started = await startChatgptLogin();
			authorizeUrl = started.authorize_url;
			loginMode = started.mode === 'manual' ? 'manual' : 'browser';
			if (started.mode === 'browser') {
				loginStatus = { ok: true, message: 'Waiting for ChatGPT sign-in in the browser…' };
			}
		} catch (error) {
			loginStatus = { ok: false, message: String(error) };
			loginMode = 'idle';
		}
	}

	async function onChatgptComplete() {
		loginStatus = null;
		try {
			await completeChatgptLogin(redirectUrl);
			redirectUrl = '';
			loginMode = 'idle';
			settings = await loadSettings();
			provider = settings.provider;
			model = settings.model;
			loginStatus = { ok: true, message: 'Signed in with ChatGPT.' };
		} catch (error) {
			loginStatus = { ok: false, message: String(error) };
		}
	}

	async function onChatgptSignOut() {
		loginStatus = null;
		try {
			await signOutChatgpt();
			settings = await loadSettings();
			provider = settings.provider;
			loginStatus = { ok: true, message: 'Signed out of ChatGPT.' };
		} catch (error) {
			loginStatus = { ok: false, message: String(error) };
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
			<div class="flex flex-row gap-2">
				<Button
					label="OpenAI Compatible"
					variant={provider === 'openai_compatible' ? 'primary' : 'ghost'}
					onclick={() => void chooseProvider('openai_compatible')}
				/>
				<Button
					label="ChatGPT Plus/Pro"
					variant={provider === 'chatgpt_codex' ? 'primary' : 'ghost'}
					onclick={() => void chooseProvider('chatgpt_codex')}
				/>
			</div>
		</div>
		{#if provider === 'openai_compatible'}
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
		{/if}
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Model</span>
			<input
				bind:value={model}
				class="rounded-md border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			/>
		</label>
		{#if provider === 'openai_compatible'}
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
		{:else}
			<div class="text-sm text-[var(--muted)]">
				Uses your ChatGPT Plus/Pro subscription through Codex. Tokens stay in Keychain.
			</div>
			{#if settings?.chatgpt_signed_in}
				<div class="text-sm">ChatGPT session stored in Keychain.</div>
				<Button label="Sign out" onclick={() => void onChatgptSignOut()} />
			{:else}
				<Button
					label="Sign in with ChatGPT"
					onclick={() => void onChatgptLogin()}
					variant="primary"
				/>
			{/if}
			{#if loginMode === 'manual'}
				<div class="text-sm text-[var(--muted)]">
					Port 1455 is busy (often Codex CLI). Open this URL, then paste the redirect:
				</div>
				<div class="text-sm break-all font-mono">{authorizeUrl}</div>
				<label class="flex flex-col gap-1">
					<span class="text-sm text-[var(--muted)]">Redirect URL</span>
					<input
						bind:value={redirectUrl}
						class="rounded-md border px-2 py-1"
						style:border-color="var(--border)"
						style:background="var(--bg)"
					/>
				</label>
				<Button label="Complete sign-in" onclick={() => void onChatgptComplete()} />
			{/if}
			{#if loginStatus}
				<div class="text-sm" style:color={loginStatus.ok ? 'var(--ok)' : 'var(--danger)'}
					>{loginStatus.message}</div
				>
			{/if}
		{/if}
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
