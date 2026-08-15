//! Filesystem and computer tools. Must not depend on GPUI, `crosspond-core`,
//! or `crosspond-macos` (macos implements `AccessibilityBackend` instead).

#![deny(unsafe_code)]

mod ax_outline;
mod computer;
mod fs_tools;
mod path;
mod registry;
mod tool;
mod workspace;

pub use ax_outline::{
    AxOutlineNode, MAX_AX_DEPTH, MAX_AX_NODES, MAX_AX_TEXT_CHARS, render_ax_outline,
    truncate_ax_text,
};
pub use computer::{AccessibilityBackend, computer_registry, register_computer_tools};
pub use fs_tools::filesystem_registry;
pub use path::{PathError, PathScope, ResolvedPath, classify_write_path, resolve_path};
pub use registry::ToolRegistry;
pub use tool::{MAX_TOOL_OUTPUT_BYTES, Tool, ToolContext, ToolDefinition, ToolError, ToolResult};
pub use workspace::{Workspace, WorkspaceError};

pub const MAX_LIST_ENTRIES: usize = 200;
