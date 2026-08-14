//! Filesystem tools. Must not depend on GPUI or `crosspond-core`.

#![deny(unsafe_code)]

mod fs_tools;
mod path;
mod registry;
mod tool;
mod workspace;

pub use fs_tools::filesystem_registry;
pub use path::{PathError, PathScope, ResolvedPath, classify_write_path, resolve_path};
pub use registry::ToolRegistry;
pub use tool::{MAX_TOOL_OUTPUT_BYTES, Tool, ToolContext, ToolDefinition, ToolError, ToolResult};
pub use workspace::{Workspace, WorkspaceError};

pub const MAX_LIST_ENTRIES: usize = 200;
