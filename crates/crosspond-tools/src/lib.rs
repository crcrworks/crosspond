//! Filesystem, computer, and web tools. Must not depend on Tauri, `crosspond-core`,
//! or `crosspond-macos` (macos implements `AccessibilityBackend` instead).

#![deny(unsafe_code)]

mod ax_outline;
mod browser;
mod browser_cdp;
mod browser_snapshot;
mod calendar;
mod computer;
mod fs_tools;
mod knowledge;
mod path;
mod registry;
mod sandbox;
mod scratch;
mod shell;
mod ssrf;
mod tool;
mod web;

pub use ax_outline::{
    AxOutlineNode, MAX_AX_DEPTH, MAX_AX_NODES, MAX_AX_TEXT_CHARS, render_ax_outline,
    truncate_ax_text,
};
pub use browser::{
    BrowserBackend, BrowserTransport, DisconnectedBrowser, EXTENSION_DISCONNECTED,
    HttpAuthChallenge, host_from_url, http_auth_required_message, http_hosts_from_note,
    is_browser_tool, is_browser_write_tool, normalize_host, parse_host_list,
    register_browser_tools, site_is_allowed, site_is_blocked,
};
pub use browser_cdp::ExtensionBrowser;
pub use calendar::{CalendarBackend, register_calendar_tools};
pub use computer::{
    AccessibilityBackend, AppBackend, InputBackend, Screenshot, ScreenshotBackend,
    computer_and_screenshot_registry, computer_and_screenshot_registry_with_browser,
    computer_registry, register_app_tools, register_computer_tools, register_input_tools,
    register_screenshot_tools,
};
pub use fs_tools::filesystem_registry;
pub use knowledge::{
    KnowledgeBackend, KnowledgeEdge, KnowledgeHit, KnowledgeRecord, register_knowledge_tools,
};
pub use path::{
    PathError, PathScope, ResolvedPath, classify_write_path, resolve_path, resolve_requested,
};
pub use registry::ToolRegistry;
pub use sandbox::{ShellSandbox, UnsandboxedShell, unsandboxed_shell, unsandboxed_shell_command};
pub use scratch::{ScratchError, ScratchReason, ScratchSpace};
pub use shell::{MAX_SHELL_OUTPUT_BYTES, command_embeds_credentials, register_shell_tools};
pub use ssrf::{
    SsrfResolver, filter_resolved_addrs, is_blocked_ip, validate_fetch_url,
    validate_fetch_url_for_hosts,
};
pub use tool::{
    ApprovalBody, MAX_TOOL_OUTPUT_BYTES, Tool, ToolContext, ToolDefinition, ToolError, ToolImage,
    ToolResult,
};
pub use web::{format_exa_results, register_web_tools, strip_html, web_tools_registry};

pub const MAX_LIST_ENTRIES: usize = 200;
