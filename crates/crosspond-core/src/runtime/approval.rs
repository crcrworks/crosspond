use std::path::Path;

use crosspond_model::ToolCall;
use crosspond_tools::{ApprovalBody, PathScope, ToolContext, classify_write_path, is_browser_tool};
use serde_json::json;

use crate::command::{ApprovalId, RuntimeCommand};
use crate::event::AgentEvent;
use crate::ids::TaskId;
use crate::network_policy::is_egress_tool;
use crate::policy::{
    AgentAsk, BrowserHostDecision, ComputerApprovalMode, PolicyDecision, RiskLevel,
    browser_host_decision, evaluate_with, risk_for_tool,
};
use crate::receipt::append_event_log;

use super::{ApprovalOutcome, ApprovalWait, Runtime, persist_allowed_browser_host};

impl Runtime {
    pub(crate) async fn await_approval_if_needed(
        &mut self,
        task_id: TaskId,
        task_dir: &Path,
        call: &ToolCall,
        input: &serde_json::Value,
        context: &mut ToolContext,
    ) -> ApprovalOutcome {
        let path = input
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or(".");
        let scope = match context.scratch.as_ref() {
            Some(scratch) => {
                classify_write_path(&scratch.root, path).unwrap_or(PathScope::External)
            }
            None if Path::new(path).is_absolute() => PathScope::External,
            None => PathScope::Workspace,
        };
        let computer_approval = self
            .config
            .load()
            .map(|config| config.computer_approval)
            .unwrap_or_default();
        if is_browser_tool(&call.name) {
            let host = self.tools.target_host(&call.name, context, input);
            let config = self.config.load().unwrap_or_default();
            match browser_host_decision(
                &call.name,
                host.as_deref(),
                &config.browser_allowed_hosts,
                &config.browser_blocked_hosts,
            ) {
                BrowserHostDecision::Blocked(host) => {
                    return ApprovalOutcome::Rejected(format!("blocked site {host}"));
                }
                BrowserHostDecision::NeedsAllow(host)
                    if computer_approval != ComputerApprovalMode::Auto =>
                {
                    let title = format!("Allow {host}");
                    let description = "The Chrome extension can read and operate this site. Page contents stay out of Settings, receipts, and logs.".into();
                    match self
                        .prompt_tool_approval(
                            task_id,
                            task_dir,
                            &call.name,
                            title,
                            description,
                            ApprovalBody::Prose,
                        )
                        .await
                    {
                        ApprovalOutcome::Allowed => {
                            persist_allowed_browser_host(self.config.as_ref(), &host);
                        }
                        other => return other,
                    }
                }
                BrowserHostDecision::NeedsAllow(_)
                | BrowserHostDecision::Skip
                | BrowserHostDecision::Allowed => {}
            }
        }
        let risk = risk_for_tool(&call.name, scope, input);
        let auto = computer_approval == ComputerApprovalMode::Auto;
        if auto
            && call.name == "run_command"
            && self
                .shell_sandbox
                .as_ref()
                .is_some_and(|sandbox| sandbox.is_enforcing())
        {
            return ApprovalOutcome::Allowed;
        }
        let tainted_egress = self.private_context && is_egress_tool(&call.name, input);
        let needs_approval = tainted_egress
            || evaluate_with(risk, computer_approval, AgentAsk::from_tool_input(input))
                == PolicyDecision::RequireApproval;
        if !needs_approval {
            if auto && matches!(risk, RiskLevel::ExternalWrite | RiskLevel::Destructive) {
                context.allow_external = true;
            }
            return ApprovalOutcome::Allowed;
        }
        let (title, description) = if tainted_egress {
            self.tainted_egress_prompt(&call.name, context, input)
        } else {
            self.tools.approval_prompt(&call.name, context, input)
        };
        let body = self.tools.approval_body(&call.name);
        match self
            .prompt_tool_approval(task_id, task_dir, &call.name, title, description, body)
            .await
        {
            ApprovalOutcome::Allowed => {
                context.allow_external = true;
                ApprovalOutcome::Allowed
            }
            other => other,
        }
    }

    fn tainted_egress_prompt(
        &self,
        name: &str,
        context: &ToolContext,
        input: &serde_json::Value,
    ) -> (String, String) {
        let host = self
            .tools
            .target_host(name, context, input)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "this site".into());
        match name {
            "browser_fill" | "browser_type" => (
                format!("Allow private task data to be sent to {host}?"),
                "Private task data will be typed into this site. Typed values stay out of Settings, receipts, and logs.".into(),
            ),
            "browser_navigate" | "browser_new_tab" => {
                let (_, url) = self.tools.approval_prompt(name, context, input);
                (
                    format!("Allow private task data to be sent to {host}?"),
                    url,
                )
            }
            _ => self.tools.approval_prompt(name, context, input),
        }
    }

    pub(crate) async fn prompt_tool_approval(
        &mut self,
        task_id: TaskId,
        task_dir: &Path,
        tool: &str,
        title: String,
        description: String,
        body: ApprovalBody,
    ) -> ApprovalOutcome {
        let approval_id = ApprovalId::new();
        append_event_log(
            task_dir,
            json!({ "type": "approval_required", "tool": tool }),
        );
        if self
            .events
            .send(AgentEvent::ApprovalRequired {
                task_id,
                approval_id,
                title,
                description,
                body,
            })
            .is_err()
        {
            return ApprovalOutcome::Cancelled { reset: false };
        }
        match self.wait_for_approval(task_id, approval_id).await {
            ApprovalWait::Approved => {
                append_event_log(
                    task_dir,
                    json!({ "type": "approval_granted", "tool": tool }),
                );
                ApprovalOutcome::Allowed
            }
            ApprovalWait::Rejected => {
                append_event_log(
                    task_dir,
                    json!({ "type": "approval_rejected", "tool": tool }),
                );
                ApprovalOutcome::Rejected(format!("The user rejected tool `{tool}`."))
            }
            ApprovalWait::Cancelled { reset } => ApprovalOutcome::Cancelled { reset },
        }
    }

    pub(crate) async fn wait_for_approval(
        &mut self,
        task_id: TaskId,
        approval_id: ApprovalId,
    ) -> ApprovalWait {
        loop {
            match self.commands.recv().await {
                None => return ApprovalWait::Cancelled { reset: false },
                Some(RuntimeCommand::Approve(id)) if id == approval_id => {
                    return ApprovalWait::Approved;
                }
                Some(RuntimeCommand::Reject(id)) if id == approval_id => {
                    return ApprovalWait::Rejected;
                }
                Some(RuntimeCommand::Cancel(id)) if id == task_id => {
                    return ApprovalWait::Cancelled { reset: false };
                }
                Some(RuntimeCommand::ResetSession) => {
                    return ApprovalWait::Cancelled { reset: true };
                }
                Some(RuntimeCommand::TestConnection) => self.spawn_test_connection(),
                Some(RuntimeCommand::TestCompat { id }) => self.spawn_test_connection_for(Some(id)),
                Some(RuntimeCommand::Approve(_))
                | Some(RuntimeCommand::Reject(_))
                | Some(RuntimeCommand::SubmitCredential { .. })
                | Some(RuntimeCommand::Cancel(_))
                | Some(RuntimeCommand::StartTask(_))
                | Some(RuntimeCommand::ResumeSession(_))
                | Some(RuntimeCommand::ReloadKnowledge) => {}
            }
        }
    }
}
