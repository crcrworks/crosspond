import { invoke } from '@tauri-apps/api/core';
import type {
	Bootstrap,
	ComputerApproval,
	ConversationView,
	HistoryItem,
	HotkeyView,
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

export function saveConfig(baseUrl: string, model: string, vaultPath: string) {
	return invoke('save_config', { baseUrl, model, vaultPath });
}

export function setLauncherHotkey(spec: string) {
	return invoke<HotkeyView>('set_launcher_hotkey', { spec });
}

export function saveSecret(kind: 'provider' | 'exa', value: string) {
	return invoke('save_secret', { kind, value });
}

export function testConnection() {
	return invoke('test_connection');
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

export function revealArtifact(name: string) {
	return invoke('reveal_artifact', { name });
}

export function revealHistoryArtifact(taskId: string, name: string) {
	return invoke('reveal_history_artifact', { taskId, name });
}

export function setUiFlags(compact: boolean, composing: boolean, inConversation: boolean) {
	return invoke('set_ui_flags', { compact, composing, inConversation });
}

export function syncLauncherSize(compact: boolean, badgeLines: number, extraHeight: number) {
	return invoke('sync_launcher_size', { compact, badgeLines, extraHeight });
}
