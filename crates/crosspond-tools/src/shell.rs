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
const LOGIN_IN_COMMAND_ERROR: &str = "do not put usernames or passwords in run_command. For HTTP basic/digest file servers, fetch_url then fetch_url with credential_ref. For Chromium browser chrome, fill_credential with only credential_ref.";

pub fn register_shell_tools(registry: &mut ToolRegistry) {
    registry.register(Arc::new(RunCommand));
    registry.register(Arc::new(OpenUrl));
}

/// True when a shell command embeds HTTP/SMB logins (curl --user, user:pass@ URLs).
pub fn command_embeds_credentials(command: &str) -> bool {
    has_embedded_url_credentials(command) || tokens_use_http_auth_cli(&rough_shell_tokens(command))
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

fn reject_embedded_credentials(command: &str) -> Result<(), ToolError> {
    if command_embeds_credentials(command) {
        Err(ToolError::Failed(LOGIN_IN_COMMAND_ERROR.into()))
    } else {
        Ok(())
    }
}

fn has_embedded_url_credentials(command: &str) -> bool {
    let mut rest = command;
    while let Some(idx) = rest.find("://") {
        let after = &rest[idx + 3..];
        let authority_end = after
            .find(['/', '?', '#', ' ', '\t', '\n', '\'', '"'])
            .unwrap_or(after.len());
        let authority = &after[..authority_end];
        if let Some(at) = authority.rfind('@') {
            let userinfo = &authority[..at];
            if userinfo.contains(':') && !userinfo.is_empty() {
                return true;
            }
        }
        if after.is_empty() {
            break;
        }
        let skip = authority_end.max(1);
        rest = &after[skip.min(after.len())..];
    }
    false
}

fn rough_shell_tokens(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in command.chars() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn program_basename(token: &str) -> String {
    token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(token)
        .trim_end_matches(".exe")
        .to_ascii_lowercase()
}

fn tokens_use_http_auth_cli(tokens: &[String]) -> bool {
    for (i, token) in tokens.iter().enumerate() {
        let base = program_basename(token);
        if (base == "curl" && curl_args_have_auth(&tokens[i + 1..]))
            || (base == "wget" && wget_args_have_auth(&tokens[i + 1..]))
        {
            return true;
        }
        if token.contains(char::is_whitespace) && command_embeds_credentials(token) {
            return true;
        }
    }
    false
}

fn curl_args_have_auth(args: &[String]) -> bool {
    for arg in args {
        let a = arg.as_str();
        if matches!(
            a,
            "--digest"
                | "--basic"
                | "--ntlm"
                | "--negotiate"
                | "--anyauth"
                | "--user"
                | "--proxy-user"
                | "-u"
                | "-U"
        ) || a.starts_with("--user=")
            || a.starts_with("--proxy-user=")
            || (a.starts_with("-u") && !a.starts_with("--") && !a.starts_with("-unix"))
        {
            return true;
        }
        if matches!(a, "|" | "||" | "&&" | ";") || a == "--" {
            break;
        }
    }
    false
}

fn wget_args_have_auth(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--user"
                | "--password"
                | "--http-user"
                | "--http-password"
                | "--ftp-user"
                | "--ftp-password"
                | "--ask-password"
        ) || arg.starts_with("--user=")
            || arg.starts_with("--password=")
            || arg.starts_with("--http-user=")
            || arg.starts_with("--http-password=")
            || arg.starts_with("--ftp-user=")
            || arg.starts_with("--ftp-password=")
    })
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
            description: "Run a shell command in the task scratch directory. stdout and stderr are returned. sudo is not allowed. Do not put usernames, passwords, curl --user/--digest, or user:pass URLs in the command; use fetch_url with credential_ref or fill_credential. The login denylist is heuristic — Authorization headers and other flags are not covered.".into(),
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
        if command_embeds_credentials(command) {
            return (
                "Run a shell command".into(),
                "refused: login credentials are not allowed in shell commands".into(),
            );
        }
        (
            "Run a shell command".into(),
            truncate_for_approval(command, APPROVAL_COMMAND_MAX),
        )
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let command = required_string(&input, "command")?;
        reject_sudo(&command)?;
        reject_embedded_credentials(&command)?;
        let cwd = context
            .scratch
            .as_ref()
            .map(|space| space.root.as_path())
            .ok_or_else(|| ToolError::Failed("no scratch space is available".into()))?;
        let output = run_shell(&command, cwd, COMMAND_TIMEOUT)?;
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
    use crate::scratch::ScratchSpace;
    use uuid::Uuid;

    fn temp_scratch() -> ScratchSpace {
        let root = std::env::temp_dir().join(format!("crosspond-shell-{}", Uuid::new_v4()));
        ScratchSpace::create(root).unwrap()
    }

    #[test]
    fn run_command_rejects_sudo() {
        let scratch = temp_scratch();
        let mut registry = ToolRegistry::new();
        register_shell_tools(&mut registry);
        let err = registry
            .execute(
                "run_command",
                &ToolContext::with_scratch(scratch.clone()),
                json!({"command": "sudo rm -rf /"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("sudo"));
        let err = registry
            .execute(
                "run_command",
                &ToolContext::with_scratch(scratch.clone()),
                json!({"command": "ls && sudo ls"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("sudo"));
        let _ = std::fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn run_command_rejects_curl_login_without_echoing_secrets() {
        let scratch = temp_scratch();
        let mut registry = ToolRegistry::new();
        register_shell_tools(&mut registry);
        let command = "curl --digest --user ngc:hunter2 --silent --head 'https://files.example.invalid/inner/lab-share/'";
        let err = registry
            .execute(
                "run_command",
                &ToolContext::with_scratch(scratch.clone()),
                json!({ "command": command }),
            )
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("fill_credential"));
        assert!(message.contains("fetch_url"));
        assert!(message.contains("credential_ref"));
        assert!(!message.contains("hunter2"));
        assert!(!message.contains("ngc:"));
        assert!(!message.contains("files.example.invalid"));
        assert!(!message.contains(command));
        let (title, description) = registry.approval_prompt(
            "run_command",
            &ToolContext::with_scratch(scratch.clone()),
            &json!({ "command": command }),
        );
        assert_eq!(title, "Run a shell command");
        assert!(description.contains("refused"));
        assert!(!description.contains("hunter2"));
        assert!(!command_embeds_credentials("curl https://example.invalid/"));
        assert!(command_embeds_credentials(
            "curl --digest -u ngc 'https://files.example.invalid/inner/lab-share/'"
        ));
        assert!(command_embeds_credentials(
            "sh -c 'curl --user ngc:hunter2 https://files.example.invalid/'"
        ));
        assert!(command_embeds_credentials(
            "open 'smb://labuser:hunter2@lab-files/share'"
        ));
        assert!(!command_embeds_credentials("ls -la"));
        let _ = std::fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn run_command_without_scratch_fails() {
        let mut registry = ToolRegistry::new();
        register_shell_tools(&mut registry);
        let err = registry
            .execute(
                "run_command",
                &ToolContext::new(),
                json!({"command": "pwd"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("scratch"));
    }

    #[test]
    fn open_url_rejects_private_http() {
        let mut registry = ToolRegistry::new();
        register_shell_tools(&mut registry);
        let err = registry
            .execute(
                "open_url",
                &ToolContext::new(),
                json!({"url": "http://127.0.0.1/"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("private") || err.to_string().contains("blocked"));
    }
}
