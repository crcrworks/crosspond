export function toolActivityLabel(name: string): string {
	switch (name) {
		case 'read_file':
			return 'Reading file…';
		case 'write_file':
			return 'Writing file…';
		case 'list_directory':
			return 'Listing directory…';
		case 'create_directory':
			return 'Creating directory…';
		case 'list_apps':
			return 'Listing apps…';
		case 'open_app':
			return 'Opening an app…';
		case 'focus_app':
			return 'Focusing an app…';
		case 'get_accessibility_snapshot':
			return 'Looking at the screen…';
		case 'take_screenshot':
			return 'Taking a screenshot…';
		case 'ui_press':
			return 'Pressing a control…';
		case 'ui_set_value':
			return 'Filling a field…';
		case 'fill_credential':
			return 'Filling a login…';
		case 'ui_click':
			return 'Clicking…';
		case 'ui_type':
			return 'Typing…';
		case 'ui_hotkey':
			return 'Sending a shortcut…';
		case 'ui_scroll':
			return 'Scrolling…';
		case 'calendar_events':
			return 'Reading the calendar…';
		case 'knowledge_search':
			return 'Searching vault…';
		case 'knowledge_find_procedure':
			return 'Finding a procedure…';
		case 'knowledge_read':
			return 'Reading a note…';
		case 'knowledge_neighbors':
			return 'Following note links…';
		case 'knowledge_backlinks':
			return 'Finding backlinks…';
		case 'knowledge_ingest':
			return 'Ingesting into vault…';
		case 'knowledge_propose_update':
			return 'Proposing a vault update…';
		case 'knowledge_read_later':
			return 'Saving for later…';
		case 'knowledge_archive_source':
			return 'Archiving a source…';
		case 'run_command':
			return 'Running a command…';
		case 'open_url':
			return 'Opening a URL…';
		case 'web_search':
			return 'Searching the web…';
		case 'fetch_url':
			return 'Fetching a page…';
		default:
			return `Running ${name}…`;
	}
}

export function toolDoneLabel(name: string): string {
	switch (name) {
		case 'read_file':
			return 'Read a file';
		case 'write_file':
			return 'Wrote a file';
		case 'list_directory':
			return 'Listed a directory';
		case 'create_directory':
			return 'Created a directory';
		case 'list_apps':
			return 'Listed apps';
		case 'open_app':
			return 'Opened an app';
		case 'focus_app':
			return 'Focused an app';
		case 'get_accessibility_snapshot':
			return 'Looked at the screen';
		case 'take_screenshot':
			return 'Took a screenshot';
		case 'ui_press':
			return 'Pressed a control';
		case 'ui_set_value':
			return 'Filled a field';
		case 'fill_credential':
			return 'Filled a login';
		case 'ui_click':
			return 'Clicked';
		case 'ui_type':
			return 'Typed';
		case 'ui_hotkey':
			return 'Sent a shortcut';
		case 'ui_scroll':
			return 'Scrolled';
		case 'calendar_events':
			return 'Read the calendar';
		case 'knowledge_search':
			return 'Searched vault';
		case 'knowledge_find_procedure':
			return 'Found a procedure';
		case 'knowledge_read':
			return 'Read a note';
		case 'knowledge_neighbors':
			return 'Followed note links';
		case 'knowledge_backlinks':
			return 'Found backlinks';
		case 'knowledge_ingest':
			return 'Ingested into vault';
		case 'knowledge_propose_update':
			return 'Proposed a vault update';
		case 'knowledge_read_later':
			return 'Saved for later';
		case 'knowledge_archive_source':
			return 'Archived a source';
		case 'run_command':
			return 'Ran a command';
		case 'open_url':
			return 'Opened a URL';
		case 'web_search':
			return 'Searched the web';
		case 'fetch_url':
			return 'Fetched a page';
		default:
			return `Ran ${name}`;
	}
}

export function toolRowLabel(name: string, summary: string): string {
	const trimmed = summary.trim();
	return trimmed.length === 0 ? name : `${name}  ${trimmed}`;
}

export type ToolTone = 'blue' | 'green' | 'yellow' | 'red' | 'muted';

export type ToolVisual = {
	icon: string;
	tone: ToolTone;
};

export function toolVisual(name: string): ToolVisual {
	switch (name) {
		case 'read_file':
			return { icon: '/icons/file.svg', tone: 'yellow' };
		case 'write_file':
			return { icon: '/icons/pencil.svg', tone: 'yellow' };
		case 'list_directory':
		case 'create_directory':
			return { icon: '/icons/folder.svg', tone: 'yellow' };
		case 'list_apps':
		case 'open_app':
		case 'focus_app':
		case 'get_accessibility_snapshot':
		case 'take_screenshot':
			return { icon: '/icons/monitor.svg', tone: 'blue' };
		case 'ui_press':
		case 'ui_click':
		case 'ui_type':
		case 'ui_hotkey':
		case 'ui_scroll':
			return { icon: '/icons/pointer.svg', tone: 'blue' };
		case 'ui_set_value':
			return { icon: '/icons/text.svg', tone: 'blue' };
		case 'fill_credential':
			return { icon: '/icons/text.svg', tone: 'yellow' };
		case 'calendar_events':
			return { icon: '/icons/calendar.svg', tone: 'yellow' };
		case 'knowledge_search':
		case 'knowledge_find_procedure':
			return { icon: '/icons/search.svg', tone: 'green' };
		case 'knowledge_read':
		case 'knowledge_neighbors':
		case 'knowledge_backlinks':
			return { icon: '/icons/file.svg', tone: 'green' };
		case 'knowledge_ingest':
		case 'knowledge_propose_update':
		case 'knowledge_read_later':
		case 'knowledge_archive_source':
			return { icon: '/icons/pencil.svg', tone: 'green' };
		case 'run_command':
			return { icon: '/icons/terminal.svg', tone: 'red' };
		case 'open_url':
		case 'web_search':
		case 'fetch_url':
			return { icon: '/icons/globe.svg', tone: 'blue' };
		default:
			return { icon: '/icons/wrench.svg', tone: 'muted' };
	}
}

export function toolIconPath(name: string): string {
	return toolVisual(name).icon;
}

export function taskStatusVisual(status: string): { label: string; tone: ToolTone } {
	switch (status) {
		case 'completed':
			return { label: 'Done', tone: 'green' };
		case 'failed':
			return { label: 'Failed', tone: 'red' };
		case 'cancelled':
			return { label: 'Cancelled', tone: 'muted' };
		case 'running':
			return { label: 'Interrupted', tone: 'yellow' };
		default:
			return { label: 'Unknown', tone: 'muted' };
	}
}

export const APPROVAL_MODES = ['auto', 'agent', 'manual'] as const;

export function approvalLabel(mode: string): string {
	switch (mode) {
		case 'auto':
			return 'Auto';
		case 'agent':
			return 'AI';
		default:
			return 'Manual';
	}
}
