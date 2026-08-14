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
