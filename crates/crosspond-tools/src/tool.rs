use serde_json::Value;
use thiserror::Error;

use crate::workspace::Workspace;

/// Truncate tool output so a directory dump cannot blow the context window.
pub const MAX_TOOL_OUTPUT_BYTES: usize = 100 * 1024;

#[derive(Clone, Debug)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone, Debug)]
pub struct ToolContext {
    pub workspace: Workspace,
    pub frontmost_name: Option<String>,
    pub frontmost_pid: Option<i32>,
    /// Set only for a single tool call after the user approved an external write.
    pub allow_external: bool,
}

impl ToolContext {
    pub fn new(workspace: Workspace) -> Self {
        Self {
            workspace,
            frontmost_name: None,
            frontmost_pid: None,
            allow_external: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolResult {
    pub text: String,
    pub created_file: Option<std::path::PathBuf>,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("{0}")]
    Failed(String),
}

pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError>;

    /// Copy for the approval card. Must not include secrets or file contents.
    fn approval_prompt(&self, _context: &ToolContext, _input: &Value) -> (String, String) {
        (format!("Run `{}`", self.definition().name), String::new())
    }
}

pub fn truncate_output(text: String) -> String {
    if text.len() <= MAX_TOOL_OUTPUT_BYTES {
        return text;
    }
    let mut cut = MAX_TOOL_OUTPUT_BYTES;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}\n… truncated after {MAX_TOOL_OUTPUT_BYTES} bytes",
        &text[..cut]
    )
}
