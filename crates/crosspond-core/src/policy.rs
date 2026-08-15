use crosspond_tools::PathScope;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiskLevel {
    ReadOnly,
    WorkspaceWrite,
    ExternalWrite,
    ComputerAction,
    Shell,
    Destructive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyDecision {
    Allow,
    RequireApproval,
}

pub fn evaluate(risk: RiskLevel) -> PolicyDecision {
    match risk {
        RiskLevel::ReadOnly | RiskLevel::WorkspaceWrite => PolicyDecision::Allow,
        RiskLevel::ExternalWrite
        | RiskLevel::ComputerAction
        | RiskLevel::Shell
        | RiskLevel::Destructive => PolicyDecision::RequireApproval,
    }
}

pub fn risk_for_tool(name: &str, scope: PathScope) -> RiskLevel {
    match (name, scope) {
        ("read_file" | "list_directory" | "get_accessibility_snapshot", _) => RiskLevel::ReadOnly,
        ("write_file" | "create_directory", PathScope::Workspace) => RiskLevel::WorkspaceWrite,
        ("write_file" | "create_directory", PathScope::External) => RiskLevel::ExternalWrite,
        ("run_command", _) => RiskLevel::Shell,
        ("ui_press" | "ui_set_value", _) => RiskLevel::ComputerAction,
        _ => RiskLevel::Destructive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TaskId;
    use crate::workspace::FsWorkspaceManager;
    use crate::workspace::WorkspaceManager;
    use crosspond_tools::classify_write_path;
    use std::fs;

    #[test]
    fn read_workspace_is_auto() {
        assert_eq!(
            evaluate(risk_for_tool("read_file", PathScope::Workspace)),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn write_workspace_is_auto() {
        assert_eq!(
            evaluate(risk_for_tool("write_file", PathScope::Workspace)),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn write_desktop_requires_approval() {
        let root = std::env::temp_dir().join(format!("crosspond-policy-{}", uuid::Uuid::new_v4()));
        let manager = FsWorkspaceManager::new(root.join("workspaces"));
        let workspace = manager.create(TaskId::new()).unwrap();
        let desktop = format!(
            "{}/Desktop/file.txt",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
        );
        let scope = classify_write_path(&workspace.root, &desktop).unwrap();
        assert_eq!(scope, PathScope::External);
        assert_eq!(
            evaluate(risk_for_tool("write_file", scope)),
            PolicyDecision::RequireApproval
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shell_requires_approval() {
        assert_eq!(
            evaluate(risk_for_tool("run_command", PathScope::Workspace)),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn computer_action_requires_approval() {
        assert_eq!(
            evaluate(risk_for_tool("ui_press", PathScope::Workspace)),
            PolicyDecision::RequireApproval
        );
        assert_eq!(
            evaluate(risk_for_tool("ui_set_value", PathScope::Workspace)),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn accessibility_snapshot_is_auto() {
        assert_eq!(
            evaluate(risk_for_tool(
                "get_accessibility_snapshot",
                PathScope::Workspace
            )),
            PolicyDecision::Allow
        );
    }
}
