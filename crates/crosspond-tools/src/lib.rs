//! Filesystem, computer, and web tools. Must not depend on GPUI, `crosspond-core`,
//! or `crosspond-macos` (macos implements `AccessibilityBackend` instead).

#![deny(unsafe_code)]

mod ax_outline;
mod calendar;
mod computer;
mod fs_tools;
mod path;
mod registry;
mod scratch;
mod shell;
mod ssrf;
mod tool;
mod web;

pub use ax_outline::{
    AxOutlineNode, MAX_AX_DEPTH, MAX_AX_NODES, MAX_AX_TEXT_CHARS, render_ax_outline,
    truncate_ax_text,
};
pub use calendar::{CalendarBackend, register_calendar_tools};
pub use computer::{
    AccessibilityBackend, AppBackend, InputBackend, Screenshot, ScreenshotBackend,
    computer_and_screenshot_registry, computer_registry, register_app_tools,
    register_computer_tools, register_input_tools, register_screenshot_tools,
};
pub use fs_tools::filesystem_registry;
pub use path::{
    PathError, PathScope, ResolvedPath, classify_write_path, resolve_path, resolve_requested,
};
pub use registry::ToolRegistry;
pub use scratch::{ScratchError, ScratchReason, ScratchSpace};
pub use shell::register_shell_tools;
pub use ssrf::{is_blocked_ip, validate_fetch_url};
pub use tool::{
    MAX_TOOL_OUTPUT_BYTES, Tool, ToolContext, ToolDefinition, ToolError, ToolImage, ToolResult,
};
pub use web::{format_exa_results, register_web_tools, strip_html, web_tools_registry};

pub const MAX_LIST_ENTRIES: usize = 200;
