<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import { loadSettings, openSystemSettings, saveConfig, saveSecret, testConnection } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import type { AgentEvent, SettingsView } from '$lib/types';
	import { onMount } from 'svelte';

	let settings = $state<SettingsView | null>(null);
	let baseUrl = $state('');
	let model = $state('');
	let apiKey = $state('');
	let exaKey = $state('');
	let saveStatus = $state<string | null>(null);
	let testStatus = $state<{ ok: boolean; message: string } | null>(null);

	onMount(() => {
		void (async () => {
			const loaded = await loadSettings();
			settings = loaded;
			baseUrl = loaded.base_url;
			model = loaded.model;
		})();
		let unlisten: (() => void) | undefined;
		void listen<AgentEvent>('agent-event', (event) => {
			if (event.payload.type === 'connection_tested') {
				testStatus = { ok: event.payload.ok, message: event.payload.message };
			}
		}).then((fn) => {
			unlisten = fn;
		});
		return () => unlisten?.();
	});

	async function persist() {
		await saveConfig(baseUrl, model);
		await saveSecret('provider', apiKey);
		await saveSecret('exa', exaKey);
		if (apiKey.trim()) apiKey = '';
		if (exaKey.trim()) exaKey = '';
		settings = await loadSettings();
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
</script>

<div class="h-full overflow-y-auto px-6 py-4 text-[var(--text)]" style:background="var(--bg)">
	<div class="flex flex-col gap-4">
		<div class="pt-2 text-sm font-semibold text-[var(--muted)]">General</div>
		<div class="flex flex-col gap-1">
			<div class="text-sm text-[var(--muted)]">Launch Crosspond</div>
			<div>Option + Space</div>
		</div>
		<div class="pt-2 text-sm font-semibold text-[var(--muted)]">AI</div>
		<div class="flex flex-col gap-1">
			<div class="text-sm text-[var(--muted)]">Provider</div>
			<div>OpenAI Compatible</div>
		</div>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Base URL</span>
			<input
				bind:value={baseUrl}
				class="rounded-sm border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			/>
		</label>
		<div class="text-sm text-[var(--muted)]">Must include /v1, e.g. http://127.0.0.1:1234/v1</div>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Model</span>
			<input
				bind:value={model}
				class="rounded-sm border px-2 py-1"
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
				class="rounded-sm border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			/>
		</label>
		<div class="pt-2 text-sm font-semibold text-[var(--muted)]">Search</div>
		<label class="flex flex-col gap-1">
			<span class="text-sm text-[var(--muted)]">Exa API Key</span>
			<input
				bind:value={exaKey}
				type="password"
				placeholder={settings?.exa_key_stored
					? '••••••••  stored in Keychain'
					: 'Optional — for web_search'}
				class="rounded-sm border px-2 py-1"
				style:border-color="var(--border)"
				style:background="var(--bg)"
			/>
		</label>
		<div class="text-sm text-[var(--muted)]">
			Required for web_search. Free credits at https://dashboard.exa.ai/api-keys
		</div>
		<div class="pt-2 text-sm font-semibold text-[var(--muted)]">Permissions</div>
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
				<div class="flex flex-col gap-1">
					<div class="flex flex-row items-center gap-2">
						<div class="text-sm text-[var(--muted)]">{row[0]}</div>
						<div class="text-sm" style:color={row[1] ? 'var(--ok)' : 'var(--muted)'}>
							{row[1] ? 'Enabled' : 'Not enabled'}
						</div>
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
			<Button label="Save" onclick={() => void onSave()} />
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
