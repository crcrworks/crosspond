use std::sync::Arc;

use serde_json::Value;
use thiserror::Error;

use crate::knowledge::KnowledgeBackend;
use crate::scratch::ScratchSpace;

/// Truncate tool output so a directory dump cannot blow the context window.
pub const MAX_TOOL_OUTPUT_BYTES: usize = 100 * 1024;

#[derive(Clone, Debug)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone, Default)]
pub struct ToolContext {
    /// Present only after the runtime lazily created a scratch space.
    pub scratch: Option<ScratchSpace>,
    pub frontmost_name: Option<String>,
    pub frontmost_pid: Option<i32>,
    /// Set only for a single tool call after the user approved an external write.
    pub allow_external: bool,
    /// Search provider API key (Exa for now). `Debug` redacts the value.
    pub search_api_key: Option<String>,
    /// Read-only Knowledge Vault lookup. Absent when no vault is configured.
    pub knowledge: Option<Arc<dyn KnowledgeBackend>>,
}

impl ToolContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_scratch(scratch: ScratchSpace) -> Self {
        Self {
            scratch: Some(scratch),
            ..Self::default()
        }
    }
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("scratch", &self.scratch)
            .field("frontmost_name", &self.frontmost_name)
            .field("frontmost_pid", &self.frontmost_pid)
            .field("allow_external", &self.allow_external)
            .field(
                "search_api_key",
                &self.search_api_key.as_ref().map(|_| "***"),
            )
            .field("knowledge", &self.knowledge.as_ref().map(|_| "set"))
            .finish()
    }
}

/// Image attached to a tool result for vision models.
///
/// `Debug` redacts pixel bytes.
#[derive(Clone, Eq, PartialEq)]
pub struct ToolImage {
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Debug for ToolImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolImage")
            .field("media_type", &self.media_type)
            .field("bytes_len", &self.bytes.len())
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct ToolResult {
    pub text: String,
    pub created_file: Option<std::path::PathBuf>,
    pub image: Option<ToolImage>,
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
