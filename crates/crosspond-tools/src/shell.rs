use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::registry::ToolRegistry;
use crate::ssrf::validate_fetch_url;
use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult, truncate_output};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(25);
const APPROVAL_COMMAND_MAX: usize = 120;

pub fn register_shell_tools(registry: &mut ToolRegistry) {
    registry.register(Arc::new(RunCommand));
    registry.register(Arc::new(OpenUrl));
}

fn required_string(input: &Value, key: &str) -> Result<String, ToolError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::Failed(format!("{key} is required")))
}

fn reject_sudo(command: &str) -> Result<(), ToolError> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(ToolError::Failed("command is required".into()));
    }
    if trimmed.starts_with("sudo") || trimmed.contains(" sudo ") {
        return Err(ToolError::Failed("sudo is not allowed".into()));
    }
    Ok(())
}

fn truncate_for_approval(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let short: String = text.chars().take(max).collect();
        format!("{short}…")
    }
}

fn run_shell(
    command: &str,
    cwd: &Path,
    timeout: Duration,
) -> Result<std::process::Output, ToolError> {
    let (tx, rx) = mpsc::channel();
    let command = command.to_string();
    let cwd = cwd.to_path_buf();
    std::thread::spawn(move || {
        let output = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        let _ = tx.send(output);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(err)) => Err(ToolError::Failed(err.to_string())),
        Err(_) => Err(ToolError::Failed(
            "command timed out after 25 seconds".into(),
        )),
    }
}

fn format_command_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut text = String::new();
    if !stdout.is_empty() {
        text.push_str(stdout.trim_end());
    }
    if !stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        if !stderr.trim().is_empty() {
            text.push_str(stderr.trim_end());
        }
    }
    if text.is_empty() {
        if output.status.success() {
            "(no output)".into()
        } else {
            format!("command failed with status {}", output.status)
        }
    } else {
        text
    }
}

fn url_approval_detail(url: &str) -> String {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        let scheme = parsed.scheme();
        let host = parsed.host_str().unwrap_or("");
        if host.is_empty() {
            scheme.to_string()
        } else {
            format!("{scheme}://{host}")
        }
    } else {
        truncate_for_approval(url, APPROVAL_COMMAND_MAX)
    }
}

struct RunCommand;

impl Tool for RunCommand {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "run_command".into(),
            description: "Run a shell command in the task workspace directory. stdout and stderr are returned. sudo is not allowed.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to run"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let command = input.get("command").and_then(Value::as_str).unwrap_or("");
        (
            "Run a shell command".into(),
            truncate_for_approval(command, APPROVAL_COMMAND_MAX),
        )
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let command = required_string(&input, "command")?;
        reject_sudo(&command)?;
        let output = run_shell(&command, &context.workspace.root, COMMAND_TIMEOUT)?;
        Ok(ToolResult {
            text: truncate_output(format_command_output(&output)),
            created_file: None,
            image: None,
        })
    }
}

struct OpenUrl;

impl Tool for OpenUrl {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "open_url".into(),
            description: "Open a URL with the system default handler (browser, Mail, Calendar, etc.). Public http(s) URLs are SSRF-checked.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL to open"
                    }
                },
                "required": ["url"]
            }),
        }
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let url = input.get("url").and_then(Value::as_str).unwrap_or("");
        ("Open URL".into(), url_approval_detail(url))
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let url = required_string(&input, "url")?;
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(ToolError::Failed("url is required".into()));
        }
        if trimmed.starts_with("file:") {
            return Err(ToolError::Failed("file: URLs are not allowed".into()));
        }
        let is_http = trimmed.starts_with("http://") || trimmed.starts_with("https://");
        if is_http {
            validate_fetch_url(trimmed)?;
        }
        let status = Command::new("open")
            .arg(trimmed)
            .status()
            .map_err(|err| ToolError::Failed(err.to_string()))?;
        if !status.success() {
            return Err(ToolError::Failed(format!(
                "open failed with status {}",
                status
            )));
        }
        Ok(ToolResult {
            text: format!("Opened {trimmed}"),
            created_file: None,
            image: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;
    use uuid::Uuid;

    fn temp_workspace() -> Workspace {
        let root = std::env::temp_dir().join(format!("crosspond-shell-{}", Uuid::new_v4()));
        Workspace::create(root).unwrap()
    }

    #[test]
    fn run_command_rejects_sudo() {
        let workspace = temp_workspace();
        let mut registry = ToolRegistry::new();
        register_shell_tools(&mut registry);
        let err = registry
            .execute(
                "run_command",
                &ToolContext::new(workspace.clone()),
                json!({"command": "sudo rm -rf /"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("sudo"));
        let err = registry
            .execute(
                "run_command",
                &ToolContext::new(workspace.clone()),
                json!({"command": "ls && sudo ls"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("sudo"));
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn open_url_rejects_private_http() {
        let workspace = temp_workspace();
        let mut registry = ToolRegistry::new();
        register_shell_tools(&mut registry);
        let err = registry
            .execute(
                "open_url",
                &ToolContext::new(workspace.clone()),
                json!({"url": "http://127.0.0.1/"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("private") || err.to_string().contains("blocked"));
        let _ = std::fs::remove_dir_all(&workspace.root);
    }
}
