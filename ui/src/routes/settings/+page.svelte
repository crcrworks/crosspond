<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import {
		addCompat,
		cancelChatgptLogin,
		completeChatgptLogin,
		deleteCompat,
		loadSettings,
		openSystemSettings,
		pauseLauncherHotkey,
		resumeLauncherHotkey,
		saveCompat,
		saveConfig,
		saveSecret,
		setLauncherHotkey,
		signOutChatgpt,
		startChatgptLogin,
		testCompatConnection
	} from '$lib/api';
	import Badge from '$lib/components/Badge.svelte';
	import Button from '$lib/components/Button.svelte';
	import HotkeyTokens from '$lib/components/HotkeyTokens.svelte';
	import { modifierTokens, specFromKeyboardEvent } from '$lib/hotkey';
	import type { AgentEvent, CompatEndpoint, HotkeyView, SettingsView } from '$lib/types';
	import { onMount } from 'svelte';

	type TabId = 'general' | 'ai' | 'knowledge' | 'search' | 'permissions';

	let settings = $state<SettingsView | null>(null);
	let tab = $state<TabId>('ai');
	let vaultPath = $state('');
	let apiKeys = $state<Record<string, string>>({});
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
	const endpoints = $derived(settings?.openai_compat ?? []);

	onMount(() => {
		void (async () => {
			await refresh();
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
				void refresh();
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

	async function refresh() {
		applySettings(await loadSettings());
	}

	function applySettings(loaded: SettingsView) {
		settings = loaded;
		vaultPath = loaded.vault_path;
		const next: Record<string, string> = {};
		for (const endpoint of loaded.openai_compat) {
			next[endpoint.id] = apiKeys[endpoint.id] ?? '';
		}
		apiKeys = next;
	}

	async function persistVault() {
		await saveConfig(vaultPath);
		if (exaKey.trim()) {
			await saveSecret('exa', exaKey);
			exaKey = '';
		}
		await refresh();
	}

	async function persistEndpoint(endpoint: CompatEndpoint) {
		const loaded = await saveCompat(endpoint.id, endpoint.name, endpoint.base_url);
		const draft = apiKeys[endpoint.id];
		if (draft?.trim()) {
			const kind = endpoint.id === 'default' ? 'provider' : `provider.${endpoint.id}`;
			await saveSecret(kind, draft);
			apiKeys[endpoint.id] = '';
		}
		applySettings(loaded);
		if (draft?.trim()) await refresh();
	}

	async function onSaveVault() {
		try {
			await persistVault();
			saveStatus = 'Saved.';
			testStatus = null;
		} catch (error) {
			saveStatus = null;
			testStatus = { ok: false, message: String(error) };
		}
	}

	async function onSaveSearch() {
		try {
			await persistVault();
			saveStatus = 'Saved.';
		} catch (error) {
			saveStatus = null;
			testStatus = { ok: false, message: String(error) };
		}
	}

	async function onSaveEndpoint(endpoint: CompatEndpoint) {
		try {
			await persistEndpoint(endpoint);
			saveStatus = 'Saved.';
			testStatus = null;
		} catch (error) {
			saveStatus = null;
			testStatus = { ok: false, message: String(error) };
		}
	}

	async function onTestEndpoint(endpoint: CompatEndpoint) {
		try {
			await persistEndpoint(endpoint);
			saveStatus = 'Saved.';
			testStatus = { ok: true, message: 'Testing connection…' };
			await testCompatConnection(endpoint.id);
		} catch (error) {
			testStatus = { ok: false, message: String(error) };
		}
	}

	async function onTestChatgpt() {
		try {
			testStatus = { ok: true, message: 'Testing connection…' };
			await testCompatConnection('chatgpt');
		} catch (error) {
			testStatus = { ok: false, message: String(error) };
		}
	}

	async function onAddCompat() {
		try {
			applySettings(await addCompat());
			saveStatus = 'Added an OpenAI Compatible endpoint.';
		} catch (error) {
			testStatus = { ok: false, message: String(error) };
		}
	}

	async function onRemoveCompat(id: string) {
		try {
			applySettings(await deleteCompat(id));
			saveStatus = 'Removed.';
		} catch (error) {
			testStatus = { ok: false, message: String(error) };
		}
	}

	async function onChatgptLogin() {
		loginStatus = null;
		try {
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
			await refresh();
			loginStatus = { ok: true, message: 'Signed in with ChatGPT.' };
		} catch (error) {
			loginStatus = { ok: false, message: String(error) };
		}
	}

	async function onChatgptCancel() {
		loginMode = 'idle';
		loginStatus = null;
		try {
			await cancelChatgptLogin();
		} catch {
			/* ignore */
		}
	}

	async function onChatgptSignOut() {
		loginStatus = null;
		try {
			await signOutChatgpt();
			await refresh();
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

	function selectTab(next: TabId) {
		tab = next;
		saveStatus = null;
		testStatus = null;
	}
</script>

<svelte:window onkeydown={onRecordKey} onblur={onWindowBlur} />

<div class="h-full overflow-y-auto px-6 py-4 text-[var(--text)]" style:background="var(--bg)">
	<div class="flex flex-col gap-4">
		<div class="settings-tabs" role="tablist" aria-label="Settings">
			<button
				type="button"
				class={['settings-tab', tab === 'general' && 'active']}
				role="tab"
				aria-selected={tab === 'general'}
				onclick={() => selectTab('general')}>General</button
			>
			<button
				type="button"
				class={['settings-tab', tab === 'ai' && 'active']}
				role="tab"
				aria-selected={tab === 'ai'}
				onclick={() => selectTab('ai')}>AI</button
			>
			<button
				type="button"
				class={['settings-tab', tab === 'knowledge' && 'active']}
				role="tab"
				aria-selected={tab === 'knowledge'}
				onclick={() => selectTab('knowledge')}>Knowledge</button
			>
			<button
				type="button"
				class={['settings-tab', tab === 'search' && 'active']}
				role="tab"
				aria-selected={tab === 'search'}
				onclick={() => selectTab('search')}>Search</button
			>
			<button
				type="button"
				class={['settings-tab', tab === 'permissions' && 'active']}
				role="tab"
				aria-selected={tab === 'permissions'}
				onclick={() => selectTab('permissions')}>Permissions</button
			>
		</div>

		{#if tab === 'general'}
			<div class="flex flex-col gap-1">
				<div class="text-sm text-[var(--muted)]">Launch Crosspond</div>
				<button
					type="button"
					class="hotkey-record"
					aria-pressed={recording}
					aria-label={recording
						? 'Press a new launch shortcut. Escape cancels.'
						: `Launch shortcut ${launcherHotkey.tokens.join(' + ')}. Click to change.`}
					onclick={() => void startRecording()}
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
		{:else if tab === 'knowledge'}
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
				Obsidian-compatible folder. Empty uses {settings?.default_vault_path ??
					'~/Documents/Crosspond'}. Created if it does not exist.
			</div>
			<Button label="Save" onclick={() => void onSaveVault()} variant="primary" />
		{:else if tab === 'ai'}
			<div class="text-sm text-[var(--muted)]">
				Sign in or add API keys here. Pick the model in the launcher.
			</div>
			<div class="surface flex flex-col gap-3">
				<div class="text-sm font-medium">ChatGPT Plus/Pro</div>
				<div class="text-sm text-[var(--muted)]">
					Uses your ChatGPT subscription through Codex. Tokens stay in Keychain.
				</div>
				{#if settings?.chatgpt_signed_in}
					<div class="text-sm">ChatGPT session stored in Keychain.</div>
					<div class="flex flex-row gap-2">
						<Button label="Sign out" onclick={() => void onChatgptSignOut()} />
						<Button label="Test Connection" onclick={() => void onTestChatgpt()} />
					</div>
				{:else}
					<Button
						label="Sign in with ChatGPT"
						onclick={() => void onChatgptLogin()}
						variant="primary"
					/>
					{#if loginMode === 'browser'}
						<Button label="Cancel" onclick={() => void onChatgptCancel()} />
					{/if}
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
			</div>
			{#each settings?.openai_compat ?? [] as endpoint (endpoint.id)}
				<div class="surface flex flex-col gap-3">
					<div class="text-sm font-medium">{endpoint.name || 'OpenAI Compatible'}</div>
					<label class="flex flex-col gap-1">
						<span class="text-sm text-[var(--muted)]">Name</span>
						<input
							bind:value={endpoint.name}
							class="rounded-md border px-2 py-1"
							style:border-color="var(--border)"
							style:background="var(--bg)"
						/>
					</label>
					<label class="flex flex-col gap-1">
						<span class="text-sm text-[var(--muted)]">Base URL</span>
						<input
							bind:value={endpoint.base_url}
							class="rounded-md border px-2 py-1"
							style:border-color="var(--border)"
							style:background="var(--bg)"
						/>
					</label>
					<div class="text-sm text-[var(--muted)]">
						Must include /v1, e.g. http://127.0.0.1:1234/v1
					</div>
					<label class="flex flex-col gap-1">
						<span class="text-sm text-[var(--muted)]">API Key</span>
						<input
							bind:value={apiKeys[endpoint.id]}
							type="password"
							placeholder={endpoint.key_stored
								? '••••••••  stored in Keychain'
								: 'Required — stored in Keychain'}
							class="rounded-md border px-2 py-1"
							style:border-color="var(--border)"
							style:background="var(--bg)"
						/>
					</label>
					<div class="flex flex-row flex-wrap gap-2">
						<Button label="Save" onclick={() => void onSaveEndpoint(endpoint)} variant="primary" />
						<Button label="Test" onclick={() => void onTestEndpoint(endpoint)} />
						{#if endpoints.length > 1}
							<Button label="Remove" onclick={() => void onRemoveCompat(endpoint.id)} />
						{/if}
					</div>
				</div>
			{/each}
			<Button label="Add OpenAI Compatible" onclick={() => void onAddCompat()} />
		{:else if tab === 'search'}
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
			<Button label="Save" onclick={() => void onSaveSearch()} variant="primary" />
		{:else}
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
								void openSystemSettings(
									row[2] as 'accessibility' | 'screen_recording' | 'calendars'
								)}
						/>
					</div>
				{/each}
			{/if}
		{/if}

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
	.settings-tabs {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 12px;
		border-bottom: 1px solid var(--border);
	}

	.settings-tab {
		border: 0;
		border-bottom: 1px solid transparent;
		background: transparent;
		padding: 6px 0;
		font: inherit;
		font-size: 12px;
		font-weight: 600;
		letter-spacing: 0.05em;
		line-height: 1.4;
		text-transform: uppercase;
		color: var(--muted);
		cursor: pointer;
	}

	.settings-tab:hover,
	.settings-tab.active {
		color: var(--text);
	}

	.settings-tab.active {
		border-bottom-color: var(--text);
	}

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
