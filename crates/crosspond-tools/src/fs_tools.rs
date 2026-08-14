use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use crate::MAX_LIST_ENTRIES;
use crate::path::{PathScope, resolve_path};
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

fn deny_external(scope: PathScope) -> Result<(), ToolError> {
    if scope == PathScope::External {
        Err(ToolError::Failed("path is outside the workspace".into()))
    } else {
        Ok(())
    }
}

struct ReadFile;

impl Tool for ReadFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".into(),
            description: "Read a UTF-8 text file from the workspace.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the workspace root"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let requested = required_string(&input, "path")?;
        let resolved = resolve_path(&context.workspace.root, &requested)
            .map_err(|err| ToolError::Failed(err.to_string()))?;
        deny_external(resolved.scope)?;
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
        })
    }
}

struct ListDirectory;

impl Tool for ListDirectory {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_directory".into(),
            description: "List files in a workspace directory. Not recursive.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory relative to the workspace root. Defaults to ."
                    }
                }
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let requested = optional_string(&input, "path").unwrap_or_else(|| ".".into());
        let resolved = resolve_path(&context.workspace.root, &requested)
            .map_err(|err| ToolError::Failed(err.to_string()))?;
        deny_external(resolved.scope)?;
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
        })
    }
}

struct WriteFile;

impl Tool for WriteFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".into(),
            description: "Write a UTF-8 text file inside the workspace.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the workspace root"
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
        let resolved = resolve_path(&context.workspace.root, &requested)
            .map_err(|err| ToolError::Failed(err.to_string()))?;
        deny_external(resolved.scope)?;
        if let Some(parent) = resolved.path.parent() {
            fs::create_dir_all(parent).map_err(map_io)?;
        }
        fs::write(&resolved.path, content).map_err(map_io)?;
        Ok(ToolResult {
            text: format!(
                "Wrote {}",
                display_relative(&context.workspace.root, &resolved.path)
            ),
            created_file: Some(resolved.path),
        })
    }
}

struct CreateDirectory;

impl Tool for CreateDirectory {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "create_directory".into(),
            description: "Create a directory inside the workspace.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory relative to the workspace root"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let requested = required_string(&input, "path")?;
        let resolved = resolve_path(&context.workspace.root, &requested)
            .map_err(|err| ToolError::Failed(err.to_string()))?;
        deny_external(resolved.scope)?;
        fs::create_dir_all(&resolved.path).map_err(map_io)?;
        Ok(ToolResult {
            text: format!(
                "Created directory {}",
                display_relative(&context.workspace.root, &resolved.path)
            ),
            created_file: None,
        })
    }
}

fn display_relative(root: &Path, path: &Path) -> String {
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
    use crate::workspace::Workspace;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use uuid::Uuid;

    fn temp_workspace() -> Workspace {
        let root = std::env::temp_dir().join(format!("crosspond-tool-{}", Uuid::new_v4()));
        Workspace::create(root).unwrap()
    }

    fn ctx(workspace: &Workspace) -> ToolContext {
        ToolContext {
            workspace: workspace.clone(),
        }
    }

    #[test]
    fn writes_and_reads_workspace_file() {
        let workspace = temp_workspace();
        let registry = filesystem_registry();
        let context = ctx(&workspace);
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
        let _ = fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn rejects_parent_escape_write() {
        let workspace = temp_workspace();
        let err = filesystem_registry()
            .execute(
                "write_file",
                &ctx(&workspace),
                json!({"path": "../escape.txt", "content": "nope"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("outside"));
        let _ = fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn missing_file_is_not_found() {
        let workspace = temp_workspace();
        let err = filesystem_registry()
            .execute(
                "read_file",
                &ctx(&workspace),
                json!({"path": "output/missing.txt"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
        let _ = fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn oversized_file_is_rejected() {
        let workspace = temp_workspace();
        let path = workspace.output.join("big.txt");
        fs::write(&path, vec![b'a'; MAX_TOOL_OUTPUT_BYTES + 1]).unwrap();
        let err = filesystem_registry()
            .execute(
                "read_file",
                &ctx(&workspace),
                json!({"path": "output/big.txt"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("larger"));
        let _ = fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn permission_denied_is_mapped() {
        let workspace = temp_workspace();
        let path = workspace.output.join("secret.txt");
        fs::write(&path, "hidden").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o000);
        fs::set_permissions(&path, permissions).unwrap();
        let err = filesystem_registry()
            .execute(
                "read_file",
                &ctx(&workspace),
                json!({"path": "output/secret.txt"}),
            )
            .unwrap_err();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o644);
        let _ = fs::set_permissions(&path, permissions);
        assert!(err.to_string().contains("permission denied"));
        let _ = fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn lists_workspace_root() {
        let workspace = temp_workspace();
        let result = filesystem_registry()
            .execute("list_directory", &ctx(&workspace), json!({}))
            .unwrap();
        assert!(result.text.contains("output/"));
        assert!(result.text.contains("work/"));
        let _ = fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn creates_workspace_directory() {
        let workspace = temp_workspace();
        filesystem_registry()
            .execute(
                "create_directory",
                &ctx(&workspace),
                json!({"path": "output/notes"}),
            )
            .unwrap();
        assert!(workspace.output.join("notes").is_dir());
        let _ = fs::remove_dir_all(&workspace.root);
    }
}
