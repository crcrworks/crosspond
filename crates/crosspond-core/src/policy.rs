use crosspond_tools::PathScope;
use serde::{Deserialize, Serialize};

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

/// How computer actions (`ui_press`, `ui_set_value`, `ui_click`) are gated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerApprovalMode {
    /// Run UI actions without asking.
    Auto,
    /// The model sets `ask_user` per call.
    Agent,
    /// Ask before every UI action.
    #[default]
    Manual,
}

impl ComputerApprovalMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::Auto => Self::Agent,
            Self::Agent => Self::Manual,
            Self::Manual => Self::Auto,
        }
    }

    pub fn button_label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Agent => "AI",
            Self::Manual => "Manual",
        }
    }
}

/// The model's `ask_user` flag on a computer-action tool call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentAsk {
    Unspecified,
    Yes,
    No,
}

impl AgentAsk {
    pub fn from_tool_input(input: &serde_json::Value) -> Self {
        match input.get("ask_user") {
            Some(serde_json::Value::Bool(true)) => Self::Yes,
            Some(serde_json::Value::Bool(false)) => Self::No,
            _ => Self::Unspecified,
        }
    }
}

pub fn evaluate(risk: RiskLevel) -> PolicyDecision {
    evaluate_with(risk, ComputerApprovalMode::Manual, AgentAsk::Unspecified)
}

pub fn evaluate_with(
    risk: RiskLevel,
    computer: ComputerApprovalMode,
    ask: AgentAsk,
) -> PolicyDecision {
    match risk {
        RiskLevel::ReadOnly | RiskLevel::WorkspaceWrite => PolicyDecision::Allow,
        RiskLevel::ComputerAction => match computer {
            ComputerApprovalMode::Auto => PolicyDecision::Allow,
            ComputerApprovalMode::Manual => PolicyDecision::RequireApproval,
            ComputerApprovalMode::Agent => match ask {
                AgentAsk::No => PolicyDecision::Allow,
                AgentAsk::Yes | AgentAsk::Unspecified => PolicyDecision::RequireApproval,
            },
        },
        RiskLevel::ExternalWrite | RiskLevel::Shell | RiskLevel::Destructive => {
            PolicyDecision::RequireApproval
        }
    }
}

pub fn risk_for_tool(name: &str, scope: PathScope) -> RiskLevel {
    match (name, scope) {
        (
            "read_file"
            | "list_directory"
            | "get_accessibility_snapshot"
            | "take_screenshot"
            | "web_search"
            | "fetch_url",
            _,
        ) => RiskLevel::ReadOnly,
        ("write_file" | "create_directory", PathScope::Workspace) => RiskLevel::WorkspaceWrite,
        ("write_file" | "create_directory", PathScope::External) => RiskLevel::ExternalWrite,
        ("run_command", _) => RiskLevel::Shell,
        ("ui_press" | "ui_set_value" | "ui_click", _) => RiskLevel::ComputerAction,
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

    #[test]
    fn take_screenshot_is_auto() {
        assert_eq!(
            evaluate(risk_for_tool("take_screenshot", PathScope::Workspace)),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn web_search_and_fetch_are_auto() {
        assert_eq!(
            evaluate(risk_for_tool("web_search", PathScope::Workspace)),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate(risk_for_tool("fetch_url", PathScope::Workspace)),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn ui_click_requires_approval() {
        assert_eq!(
            evaluate(risk_for_tool("ui_click", PathScope::Workspace)),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn auto_computer_actions_skip_approval() {
        assert_eq!(
            evaluate_with(
                RiskLevel::ComputerAction,
                ComputerApprovalMode::Auto,
                AgentAsk::Unspecified
            ),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate_with(
                RiskLevel::ExternalWrite,
                ComputerApprovalMode::Auto,
                AgentAsk::No
            ),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn agent_computer_actions_follow_ask_user() {
        assert_eq!(
            evaluate_with(
                RiskLevel::ComputerAction,
                ComputerApprovalMode::Agent,
                AgentAsk::No
            ),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate_with(
                RiskLevel::ComputerAction,
                ComputerApprovalMode::Agent,
                AgentAsk::Yes
            ),
            PolicyDecision::RequireApproval
        );
        assert_eq!(
            evaluate_with(
                RiskLevel::ComputerAction,
                ComputerApprovalMode::Agent,
                AgentAsk::Unspecified
            ),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn agent_ask_parses_tool_input() {
        assert_eq!(
            AgentAsk::from_tool_input(&serde_json::json!({"ask_user": false})),
            AgentAsk::No
        );
        assert_eq!(
            AgentAsk::from_tool_input(&serde_json::json!({"node_id": 4})),
            AgentAsk::Unspecified
        );
    }

    #[test]
    fn computer_approval_mode_cycles() {
        assert_eq!(
            ComputerApprovalMode::Auto.cycle(),
            ComputerApprovalMode::Agent
        );
        assert_eq!(
            ComputerApprovalMode::Agent.cycle(),
            ComputerApprovalMode::Manual
        );
        assert_eq!(
            ComputerApprovalMode::Manual.cycle(),
            ComputerApprovalMode::Auto
        );
    }
}
