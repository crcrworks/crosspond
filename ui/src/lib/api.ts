import { invoke } from '@tauri-apps/api/core';
import type {
	Bootstrap,
	ComputerApproval,
	ConversationView,
	HistoryItem,
	HotkeyView,
	ModelsCatalog,
	SelectedModel,
	SettingsView,
	StartTaskResult
} from './types';
import type { Mention } from './mentions';

export function bootstrap() {
	return invoke<Bootstrap>('bootstrap');
}

export function startTask(prompt: string, mentions: Mention[] = []) {
	return invoke<StartTaskResult>('start_task', { prompt, mentions });
}

export function listMentionApps() {
	return invoke<string[]>('list_mention_apps');
}

export function approve(id: string) {
	return invoke('approve', { id });
}

export function reject(id: string) {
	return invoke('reject', { id });
}

export function cancel() {
	return invoke('cancel');
}

export function resetSession() {
	return invoke('reset_session');
}

export function hideLauncher() {
	return invoke('hide_launcher');
}

export function openSettings() {
	return invoke('open_settings');
}

export function loadSettings() {
	return invoke<SettingsView>('load_settings');
}

export function saveConfig(vaultPath: string) {
	return invoke('save_config', { vaultPath });
}

export function saveCompat(id: string, name: string, baseUrl: string) {
	return invoke<SettingsView>('save_compat', { id, name, baseUrl });
}

export function addCompat() {
	return invoke<SettingsView>('add_compat');
}

export function deleteCompat(id: string) {
	return invoke<SettingsView>('delete_compat', { id });
}

export function saveSelected(source: string, model: string) {
	return invoke<SelectedModel>('save_selected', { source, model });
}

export function saveEffort(effort: string) {
	return invoke<string>('save_effort', { effort });
}

export function listModels() {
	return invoke<ModelsCatalog>('list_models');
}

export function startChatgptLogin() {
	return invoke<{ mode: 'browser' | 'manual'; authorize_url: string }>('start_chatgpt_login');
}

export function completeChatgptLogin(redirect: string) {
	return invoke('complete_chatgpt_login', { redirect });
}

export function signOutChatgpt() {
	return invoke('sign_out_chatgpt');
}

export function setLauncherHotkey(spec: string) {
	return invoke<HotkeyView>('set_launcher_hotkey', { spec });
}

export function pauseLauncherHotkey() {
	return invoke('pause_launcher_hotkey');
}

export function resumeLauncherHotkey() {
	return invoke<HotkeyView>('resume_launcher_hotkey');
}

export function saveSecret(kind: string, value: string) {
	return invoke('save_secret', { kind, value });
}

export function testConnection() {
	return invoke('test_connection');
}

export function testCompatConnection(id: string) {
	return invoke('test_compat_connection', { id });
}

export function listHistory() {
	return invoke<HistoryItem[]>('list_history');
}

export function openConversation(id: string) {
	return invoke<ConversationView>('open_conversation', { id });
}

export function cycleComputerApproval() {
	return invoke<ComputerApproval>('cycle_computer_approval');
}

export function setComputerApproval(mode: ComputerApproval) {
	return invoke<ComputerApproval>('set_computer_approval', { mode });
}

export function permissions() {
	return invoke<SettingsView['permissions']>('permissions');
}

export function openSystemSettings(kind: 'accessibility' | 'screen_recording' | 'calendars') {
	return invoke('open_system_settings', { kind });
}

export function openExternalUrl(url: string) {
	return invoke('open_external_url', { url });
}

export function revealArtifact(name: string) {
	return invoke('reveal_artifact', { name });
}

export function revealHistoryArtifact(taskId: string, name: string) {
	return invoke('reveal_history_artifact', { taskId, name });
}

export function setUiFlags(
	compact: boolean,
	composing: boolean,
	inConversation: boolean,
	onboarding: boolean
) {
	return invoke('set_ui_flags', { compact, composing, inConversation, onboarding });
}

export function syncLauncherSize(compact: boolean, badgeLines: number, extraHeight: number) {
	return invoke('sync_launcher_size', { compact, badgeLines, extraHeight });
}
