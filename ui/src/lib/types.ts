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
	| { type: 'artifact_created'; task_id: string; display_name: string }
	| { type: 'task_completed'; task_id: string; summary: string; receipt: Receipt }
	| { type: 'task_failed'; task_id: string; message: string }
	| { type: 'task_cancelled'; task_id: string }
	| { type: 'connection_tested'; ok: boolean; message: string };

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

export type SettingsView = {
	base_url: string;
	model: string;
	provider_key_stored: boolean;
	exa_key_stored: boolean;
	permissions: {
		accessibility: boolean;
		screen_recording: boolean;
		calendars: boolean;
	};
	computer_approval: ComputerApproval;
};

export type Bootstrap = {
	needs_onboarding: boolean;
	computer_approval: ComputerApproval;
	badges: string[];
	visible: boolean;
};

export type LauncherShown = {
	badges: string[];
	onboarding: boolean;
	ready: boolean;
	visible: boolean;
};

export type WindowState =
	| 'idle'
	| 'preparing_context'
	| 'running'
	| 'waiting_approval'
	| 'completed'
	| 'failed'
	| 'cancelled';
