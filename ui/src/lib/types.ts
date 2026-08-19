import type { TranscriptBlock } from './transcript';

export type ComputerApproval = 'auto' | 'agent' | 'manual';

export type AgentEvent =
	| { type: 'task_started'; task_id: string; prompt: string }
	| { type: 'context_collected'; task_id: string }
	| { type: 'assistant_delta'; task_id: string; text: string }
	| { type: 'reasoning_delta'; task_id: string; text: string }
	| { type: 'tool_started'; task_id: string; tool: string; summary: string }
	| { type: 'tool_finished'; task_id: string; tool: string }
	| {
			type: 'approval_required';
			task_id: string;
			approval_id: string;
			title: string;
			description: string;
	  }
	| {
			type: 'credential_required';
			task_id: string;
			approval_id: string;
			title: string;
			credential_ref: string;
			save_offered: boolean;
	  }
	| { type: 'artifact_created'; task_id: string; display_name: string }
	| { type: 'task_completed'; task_id: string; summary: string; receipt: Receipt }
	| { type: 'task_failed'; task_id: string; message: string }
	| { type: 'task_cancelled'; task_id: string }
	| { type: 'connection_tested'; source: string; ok: boolean; message: string };

export type Receipt = {
	task_id: string;
	summary: string;
	actions: string[];
	artifacts: string[];
};

export type HistoryItem = {
	id: string;
	title: string;
	status: string;
	status_mark: string;
	group: string;
	receipt: Receipt | null;
	artifact_names: string[];
};

export type ConversationView = {
	id: string;
	status: string;
	transcript: TranscriptBlock[];
	receipt: Receipt | null;
	artifact_names: string[];
};

export type StartTaskResult = {
	task_id: string;
	conversation_id: string;
};

export type HotkeyView = {
	spec: string;
	tokens: string[];
};

export type SelectedModel = {
	source: string;
	model: string;
};

export type CompatEndpoint = {
	id: string;
	name: string;
	base_url: string;
	key_stored: boolean;
};

export type ListedModel = {
	id: string;
	label: string;
};

export type ModelGroup = {
	source: string;
	label: string;
	models: ListedModel[];
};

export type ModelsCatalog = {
	groups: ModelGroup[];
	selected: SelectedModel;
	reasoning_effort: ReasoningEffort;
};

export type ReasoningEffort = 'none' | 'low' | 'medium' | 'high' | 'xhigh';

export type SettingsView = {
	openai_compat: CompatEndpoint[];
	selected: SelectedModel;
	reasoning_effort: ReasoningEffort;
	vault_path: string;
	default_vault_path: string;
	chatgpt_signed_in: boolean;
	provider_ready: boolean;
	selected_ready: boolean;
	exa_key_stored: boolean;
	permissions: {
		accessibility: boolean;
		screen_recording: boolean;
		calendars: boolean;
	};
	computer_approval: ComputerApproval;
	launcher_hotkey: HotkeyView;
};

export type Bootstrap = {
	needs_onboarding: boolean;
	computer_approval: ComputerApproval;
	launcher_hotkey: HotkeyView;
	badges: string[];
	visible: boolean;
	selected: SelectedModel;
	reasoning_effort: ReasoningEffort;
};

export type LauncherShown = {
	badges: string[];
	onboarding: boolean;
	ready: boolean;
	visible: boolean;
	launcher_hotkey: HotkeyView;
};

export type WindowState =
	| 'idle'
	| 'preparing_context'
	| 'running'
	| 'waiting_approval'
	| 'completed'
	| 'failed'
	| 'cancelled';
