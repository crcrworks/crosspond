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

/// How tool approvals are gated (launcher chip: Auto / AI / Manual).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerApprovalMode {
    /// Run every tool without asking, including shell, external files, and UI.
    Auto,
    /// The model sets `ask_user` per computer-action call. Shell and external
    /// paths still require Allow.
    Agent,
    /// Ask before every UI action, shell command, and external path.
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
        RiskLevel::ExternalWrite | RiskLevel::Shell | RiskLevel::Destructive => match computer {
            ComputerApprovalMode::Auto => PolicyDecision::Allow,
            ComputerApprovalMode::Agent | ComputerApprovalMode::Manual => {
                PolicyDecision::RequireApproval
            }
        },
    }
}

pub fn risk_for_tool(name: &str, scope: PathScope, input: &serde_json::Value) -> RiskLevel {
    match name {
        "open_url" => {
            let url = input
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim();
            if url.starts_with("http://") || url.starts_with("https://") {
                RiskLevel::ReadOnly
            } else {
                RiskLevel::Shell
            }
        }
        "list_apps"
        | "calendar_events"
        | "knowledge_search"
        | "knowledge_read"
        | "knowledge_neighbors"
        | "knowledge_backlinks"
        | "knowledge_find_procedure" => RiskLevel::ReadOnly,
        "knowledge_ingest" | "knowledge_propose_update" => RiskLevel::WorkspaceWrite,
        "knowledge_read_later" | "knowledge_archive_source" => RiskLevel::WorkspaceWrite,
        "open_app" | "focus_app" | "ui_type" | "ui_hotkey" | "ui_scroll" => {
            RiskLevel::ComputerAction
        }
        _ => risk_for_tool_scope(name, scope),
    }
}

fn risk_for_tool_scope(name: &str, scope: PathScope) -> RiskLevel {
    match (name, scope) {
        ("get_accessibility_snapshot" | "take_screenshot" | "web_search" | "fetch_url", _) => {
            RiskLevel::ReadOnly
        }
        ("read_file" | "list_directory", PathScope::Workspace) => RiskLevel::ReadOnly,
        ("read_file" | "list_directory", PathScope::External) => RiskLevel::ExternalWrite,
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
    use crosspond_tools::{ScratchSpace, classify_write_path};
    use serde_json::json;
    use std::fs;

    fn empty_input() -> serde_json::Value {
        json!({})
    }

    #[test]
    fn read_workspace_is_auto() {
        assert_eq!(
            evaluate(risk_for_tool(
                "read_file",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn write_workspace_is_auto() {
        assert_eq!(
            evaluate(risk_for_tool(
                "write_file",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn write_desktop_requires_approval() {
        let root = std::env::temp_dir().join(format!("crosspond-policy-{}", uuid::Uuid::new_v4()));
        let space = ScratchSpace::create(root.join("scratch")).unwrap();
        let desktop = format!(
            "{}/Desktop/file.txt",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
        );
        let scope = classify_write_path(&space.root, &desktop).unwrap();
        assert_eq!(scope, PathScope::External);
        assert_eq!(
            evaluate(risk_for_tool("write_file", scope, &empty_input())),
            PolicyDecision::RequireApproval
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shell_requires_approval() {
        assert_eq!(
            evaluate(risk_for_tool(
                "run_command",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn computer_action_requires_approval() {
        assert_eq!(
            evaluate(risk_for_tool(
                "ui_press",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::RequireApproval
        );
        assert_eq!(
            evaluate(risk_for_tool(
                "ui_set_value",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn accessibility_snapshot_is_auto() {
        assert_eq!(
            evaluate(risk_for_tool(
                "get_accessibility_snapshot",
                PathScope::Workspace,
                &empty_input(),
            )),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn take_screenshot_is_auto() {
        assert_eq!(
            evaluate(risk_for_tool(
                "take_screenshot",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn web_search_and_fetch_are_auto() {
        assert_eq!(
            evaluate(risk_for_tool(
                "web_search",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate(risk_for_tool(
                "fetch_url",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn list_apps_calendar_and_knowledge_are_auto() {
        assert_eq!(
            evaluate(risk_for_tool(
                "list_apps",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate(risk_for_tool(
                "calendar_events",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
        for tool in [
            "knowledge_search",
            "knowledge_read",
            "knowledge_neighbors",
            "knowledge_backlinks",
            "knowledge_find_procedure",
        ] {
            assert_eq!(
                evaluate(risk_for_tool(tool, PathScope::Workspace, &empty_input())),
                PolicyDecision::Allow,
                "{tool}"
            );
        }
        assert_eq!(
            evaluate(risk_for_tool(
                "knowledge_ingest",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate(risk_for_tool(
                "knowledge_propose_update",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate(risk_for_tool(
                "knowledge_read_later",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate(risk_for_tool(
                "knowledge_archive_source",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn open_app_is_computer_action() {
        assert_eq!(
            risk_for_tool("open_app", PathScope::Workspace, &empty_input()),
            RiskLevel::ComputerAction
        );
        assert_eq!(
            risk_for_tool("ui_hotkey", PathScope::Workspace, &empty_input()),
            RiskLevel::ComputerAction
        );
    }

    #[test]
    fn open_url_http_is_auto_other_schemes_need_approval() {
        assert_eq!(
            evaluate(risk_for_tool(
                "open_url",
                PathScope::Workspace,
                &json!({"url": "https://example.com"})
            )),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate(risk_for_tool(
                "open_url",
                PathScope::Workspace,
                &json!({"url": "mailto:a@b.com"})
            )),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn ui_click_requires_approval() {
        assert_eq!(
            evaluate(risk_for_tool(
                "ui_click",
                PathScope::Workspace,
                &empty_input()
            )),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn auto_runs_every_tool_without_asking() {
        for risk in [
            RiskLevel::ComputerAction,
            RiskLevel::ExternalWrite,
            RiskLevel::Shell,
            RiskLevel::Destructive,
        ] {
            assert_eq!(
                evaluate_with(risk, ComputerApprovalMode::Auto, AgentAsk::Unspecified),
                PolicyDecision::Allow,
                "{risk:?}"
            );
        }
        assert_eq!(
            evaluate_with(RiskLevel::Shell, ComputerApprovalMode::Manual, AgentAsk::No),
            PolicyDecision::RequireApproval
        );
        assert_eq!(
            evaluate_with(
                RiskLevel::ExternalWrite,
                ComputerApprovalMode::Agent,
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
