use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::knowledge::KnowledgeBackend;
use crate::sandbox::ShellSandbox;
use crate::scratch::ScratchSpace;
use crate::skill_types::{PreparedSkillInstall, SkillEndpoints};

/// Truncate tool output so a directory dump cannot blow the context window.
pub const MAX_TOOL_OUTPUT_BYTES: usize = 100 * 1024;

/// How the launcher should render an Allow card body.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalBody {
    #[default]
    Prose,
    Command,
}

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
    /// Host-injected username for `fill_credential`. `Debug` redacts the value.
    pub fill_username: Option<String>,
    /// Host-injected password for `fill_credential`. `Debug` redacts the value.
    pub fill_password: Option<String>,
    /// http(s) hosts listed on the Resource note for this `credential_ref`.
    pub credential_hosts: Vec<String>,
    /// Host or app name shown on Allow / login cards. Never a secret.
    pub credential_destination: Option<String>,
    /// Read-only Knowledge Vault lookup. Absent when no vault is configured.
    pub knowledge: Option<Arc<dyn KnowledgeBackend>>,
    /// Set by the runtime when the user cancels the in-flight tool.
    pub cancel: Arc<AtomicBool>,
    /// Host sandbox for `run_command`. Absent means unsandboxed `sh -c`.
    pub shell_sandbox: Option<Arc<dyn ShellSandbox>>,
    /// Installed skills directory. Defaults to `~/.crosspond/skills`.
    pub skills_root: Option<PathBuf>,
    /// Global skills directory. Defaults to `~/.agents/skills`.
    pub global_skills_root: Option<PathBuf>,
    /// Test override for skills.sh / GitHub URLs.
    pub skill_endpoints: Option<SkillEndpoints>,
    /// Fetched skill waiting to be written after Allow.
    pub pending_skill_install: Option<Arc<PreparedSkillInstall>>,
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
            .field("fill_username", &self.fill_username.as_ref().map(|_| "***"))
            .field("fill_password", &self.fill_password.as_ref().map(|_| "***"))
            .field("credential_hosts", &self.credential_hosts)
            .field("credential_destination", &self.credential_destination)
            .field("knowledge", &self.knowledge.as_ref().map(|_| "set"))
            .field(
                "cancel",
                &self.cancel.load(std::sync::atomic::Ordering::Relaxed),
            )
            .field(
                "shell_sandbox",
                &self
                    .shell_sandbox
                    .as_ref()
                    .map(|sandbox| sandbox.is_enforcing()),
            )
            .field("skills_root", &self.skills_root)
            .field("global_skills_root", &self.global_skills_root)
            .field(
                "skill_endpoints",
                &self.skill_endpoints.as_ref().map(|_| "set"),
            )
            .field(
                "pending_skill_install",
                &self.pending_skill_install.as_ref().map(|_| "set"),
            )
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

    fn approval_body(&self) -> ApprovalBody {
        ApprovalBody::Prose
    }

    /// Registrable site host for browser tools (no URL path or query).
    fn target_host(&self, _context: &ToolContext, _input: &Value) -> Option<String> {
        None
    }

    /// Drop a paused Chromium HTTP auth challenge without sending credentials.
    fn abort_http_auth(&self) {}
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
