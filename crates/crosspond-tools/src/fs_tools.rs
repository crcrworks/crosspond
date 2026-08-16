use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use crate::MAX_LIST_ENTRIES;
use crate::path::{PathScope, resolve_requested};
use crate::registry::ToolRegistry;
use crate::tool::{
    MAX_TOOL_OUTPUT_BYTES, Tool, ToolContext, ToolDefinition, ToolError, ToolResult,
    truncate_output,
};

pub fn filesystem_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(std::sync::Arc::new(ReadFile));
    registry.register(std::sync::Arc::new(ListDirectory));
    registry.register(std::sync::Arc::new(WriteFile));
    registry.register(std::sync::Arc::new(CreateDirectory));
    registry
}

fn required_string(input: &Value, key: &str) -> Result<String, ToolError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::Failed(format!("{key} is required")))
}

fn optional_string(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn deny_external(scope: PathScope, allow_external: bool) -> Result<(), ToolError> {
    if scope == PathScope::External && !allow_external {
        Err(ToolError::Failed(
            "path is outside the scratch space".into(),
        ))
    } else {
        Ok(())
    }
}

fn scratch_root(context: &ToolContext) -> Option<&Path> {
    context.scratch.as_ref().map(|space| space.root.as_path())
}

struct ReadFile;

impl Tool for ReadFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".into(),
            description: "Read a UTF-8 text file from the scratch space. Absolute Mac paths outside the scratch space require user Allow.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the scratch root, or an absolute Mac path"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let requested = required_string(&input, "path")?;
        let resolved = resolve_requested(scratch_root(context), &requested)
            .map_err(|err| ToolError::Failed(err.to_string()))?;
        deny_external(resolved.scope, context.allow_external)?;
        let metadata = fs::metadata(&resolved.path).map_err(map_io)?;
        if metadata.is_dir() {
            return Err(ToolError::Failed("path is a directory".into()));
        }
        if metadata.len() > MAX_TOOL_OUTPUT_BYTES as u64 {
            return Err(ToolError::Failed(format!(
                "file is larger than {MAX_TOOL_OUTPUT_BYTES} bytes"
            )));
        }
        let bytes = fs::read(&resolved.path).map_err(map_io)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| ToolError::Failed("file is not valid UTF-8".into()))?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("a file");
        (
            "Read a file outside the scratch space".into(),
            path.to_string(),
        )
    }
}

struct ListDirectory;

impl Tool for ListDirectory {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_directory".into(),
            description: "List files in a scratch-space directory. Not recursive. Absolute Mac paths outside the scratch space require user Allow.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory relative to the scratch root. Defaults to ."
                    }
                }
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let requested = optional_string(&input, "path").unwrap_or_else(|| ".".into());
        let resolved = resolve_requested(scratch_root(context), &requested)
            .map_err(|err| ToolError::Failed(err.to_string()))?;
        deny_external(resolved.scope, context.allow_external)?;
        let metadata = fs::metadata(&resolved.path).map_err(map_io)?;
        if !metadata.is_dir() {
            return Err(ToolError::Failed("path is not a directory".into()));
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(&resolved.path).map_err(map_io)? {
            let entry = entry.map_err(map_io)?;
            let mut name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                name.push('/');
            }
            names.push(name);
        }
        names.sort();
        if names.len() > MAX_LIST_ENTRIES {
            names.truncate(MAX_LIST_ENTRIES);
            names.push("… truncated".into());
        }
        Ok(ToolResult {
            text: if names.is_empty() {
                "(empty)".into()
            } else {
                names.join("\n")
            },
            created_file: None,
            image: None,
        })
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("a directory");
        (
            "List a directory outside the scratch space".into(),
            path.to_string(),
        )
    }
}

struct WriteFile;

impl Tool for WriteFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".into(),
            description: "Write a UTF-8 text file inside the scratch space.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the scratch root"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full file contents"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let requested = required_string(&input, "path")?;
        let content = input
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("content is required".into()))?;
        let resolved = resolve_requested(scratch_root(context), &requested)
            .map_err(|err| ToolError::Failed(err.to_string()))?;
        deny_external(resolved.scope, context.allow_external)?;
        if let Some(parent) = resolved.path.parent() {
            fs::create_dir_all(parent).map_err(map_io)?;
        }
        fs::write(&resolved.path, content).map_err(map_io)?;
        Ok(ToolResult {
            text: format!(
                "Wrote {}",
                display_relative(scratch_root(context), &resolved.path)
            ),
            created_file: Some(resolved.path),
            image: None,
        })
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("a file");
        (
            "Write a file outside the scratch space".into(),
            path.to_string(),
        )
    }
}

struct CreateDirectory;

impl Tool for CreateDirectory {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "create_directory".into(),
            description: "Create a directory inside the scratch space.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory relative to the scratch root"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let requested = required_string(&input, "path")?;
        let resolved = resolve_requested(scratch_root(context), &requested)
            .map_err(|err| ToolError::Failed(err.to_string()))?;
        deny_external(resolved.scope, context.allow_external)?;
        fs::create_dir_all(&resolved.path).map_err(map_io)?;
        Ok(ToolResult {
            text: format!(
                "Created directory {}",
                display_relative(scratch_root(context), &resolved.path)
            ),
            created_file: None,
            image: None,
        })
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("a directory");
        (
            "Create a directory outside the scratch space".into(),
            path.to_string(),
        )
    }
}

fn display_relative(root: Option<&Path>, path: &Path) -> String {
    let Some(root) = root else {
        return path.display().to_string();
    };
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn map_io(err: std::io::Error) -> ToolError {
    match err.kind() {
        std::io::ErrorKind::NotFound => ToolError::Failed("file not found".into()),
        std::io::ErrorKind::PermissionDenied => ToolError::Failed("permission denied".into()),
        _ => ToolError::Failed(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::ScratchSpace;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use uuid::Uuid;

    fn temp_scratch() -> ScratchSpace {
        let root = std::env::temp_dir().join(format!("crosspond-tool-{}", Uuid::new_v4()));
        ScratchSpace::create(root).unwrap()
    }

    fn ctx(scratch: &ScratchSpace) -> ToolContext {
        ToolContext::with_scratch(scratch.clone())
    }

    #[test]
    fn writes_and_reads_scratch_file() {
        let scratch = temp_scratch();
        let registry = filesystem_registry();
        let context = ctx(&scratch);
        registry
            .execute(
                "write_file",
                &context,
                json!({"path": "output/hello.txt", "content": "hi"}),
            )
            .unwrap();
        let result = registry
            .execute("read_file", &context, json!({"path": "output/hello.txt"}))
            .unwrap();
        assert_eq!(result.text, "hi");
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn rejects_parent_escape_write() {
        let scratch = temp_scratch();
        let err = filesystem_registry()
            .execute(
                "write_file",
                &ctx(&scratch),
                json!({"path": "../escape.txt", "content": "nope"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("outside"));
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn missing_file_is_not_found() {
        let scratch = temp_scratch();
        let err = filesystem_registry()
            .execute(
                "read_file",
                &ctx(&scratch),
                json!({"path": "output/missing.txt"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn oversized_file_is_rejected() {
        let scratch = temp_scratch();
        let path = scratch.output.join("big.txt");
        fs::write(&path, vec![b'a'; MAX_TOOL_OUTPUT_BYTES + 1]).unwrap();
        let err = filesystem_registry()
            .execute(
                "read_file",
                &ctx(&scratch),
                json!({"path": "output/big.txt"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("larger"));
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn permission_denied_is_mapped() {
        let scratch = temp_scratch();
        let path = scratch.output.join("secret.txt");
        fs::write(&path, "hidden").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o000);
        fs::set_permissions(&path, permissions).unwrap();
        let err = filesystem_registry()
            .execute(
                "read_file",
                &ctx(&scratch),
                json!({"path": "output/secret.txt"}),
            )
            .unwrap_err();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o644);
        let _ = fs::set_permissions(&path, permissions);
        assert!(err.to_string().contains("permission denied"));
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn lists_scratch_root() {
        let scratch = temp_scratch();
        let result = filesystem_registry()
            .execute("list_directory", &ctx(&scratch), json!({}))
            .unwrap();
        assert!(result.text.contains("output/"));
        assert!(result.text.contains("work/"));
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn creates_scratch_directory() {
        let scratch = temp_scratch();
        filesystem_registry()
            .execute(
                "create_directory",
                &ctx(&scratch),
                json!({"path": "output/notes"}),
            )
            .unwrap();
        assert!(scratch.output.join("notes").is_dir());
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn approved_external_write_is_allowed() {
        let scratch = temp_scratch();
        let target =
            std::env::temp_dir().join(format!("crosspond-approved-{}.txt", Uuid::new_v4()));
        let mut context = ctx(&scratch);
        context.allow_external = true;
        filesystem_registry()
            .execute(
                "write_file",
                &context,
                json!({"path": target.to_string_lossy(), "content": "ok"}),
            )
            .unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "ok");
        let _ = fs::remove_file(&target);
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn external_read_file_requires_allow_external() {
        let scratch = temp_scratch();
        let target =
            std::env::temp_dir().join(format!("crosspond-read-ext-{}.txt", Uuid::new_v4()));
        fs::write(&target, "external").unwrap();
        let registry = filesystem_registry();
        let context = ctx(&scratch);
        let err = registry
            .execute(
                "read_file",
                &context,
                json!({"path": target.to_string_lossy()}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("outside"));
        let mut approved = context;
        approved.allow_external = true;
        let result = registry
            .execute(
                "read_file",
                &approved,
                json!({"path": target.to_string_lossy()}),
            )
            .unwrap();
        assert_eq!(result.text, "external");
        let _ = fs::remove_file(&target);
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn relative_read_without_scratch_fails() {
        let err = filesystem_registry()
            .execute(
                "read_file",
                &ToolContext::new(),
                json!({"path": "output/hello.txt"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("scratch"));
    }

    #[test]
    fn absolute_read_without_scratch_is_external() {
        let target =
            std::env::temp_dir().join(format!("crosspond-no-scratch-{}.txt", Uuid::new_v4()));
        fs::write(&target, "external").unwrap();
        let registry = filesystem_registry();
        let err = registry
            .execute(
                "read_file",
                &ToolContext::new(),
                json!({"path": target.to_string_lossy()}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("outside"));
        let mut approved = ToolContext::new();
        approved.allow_external = true;
        let result = registry
            .execute(
                "read_file",
                &approved,
                json!({"path": target.to_string_lossy()}),
            )
            .unwrap();
        assert_eq!(result.text, "external");
        let _ = fs::remove_file(&target);
    }
}
