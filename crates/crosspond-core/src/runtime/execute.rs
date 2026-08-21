use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crosspond_model::ImagePart;
use crosspond_tools::ToolContext;
use serde_json::json;

use crate::command::RuntimeCommand;
use crate::event::AgentEvent;
use crate::ids::TaskId;
use crate::privacy::redact_known_values;
use crate::receipt::{append_event_log, tool_ui_summary};

use super::{DEFAULT_TOOL_TIMEOUT, Runtime, ToolExec};

impl Runtime {
    pub(crate) async fn capture_mention_screenshot(
        &mut self,
        task_id: TaskId,
        task_dir: &Path,
        app: Option<&str>,
    ) -> Result<ImagePart, String> {
        let mut input = json!({});
        if let Some(app) = app.filter(|name| !name.is_empty()) {
            input["app"] = json!(app);
        }
        let summary = tool_ui_summary("take_screenshot", &input);
        append_event_log(
            task_dir,
            json!({
                "type": "tool_started",
                "tool": "take_screenshot",
                "summary": summary,
            }),
        );
        let _ = self.events.send(AgentEvent::ToolStarted {
            task_id,
            tool: "take_screenshot".into(),
            summary,
        });
        let context = self.tool_context();
        let exec = self
            .execute_tool(task_id, "take_screenshot".into(), context, input.clone())
            .await;
        match exec {
            ToolExec::Cancelled { reset } => Err(if reset {
                "cancelled:reset".into()
            } else {
                "cancelled".into()
            }),
            ToolExec::Done {
                text,
                image,
                success,
                ..
            } => {
                append_event_log(
                    task_dir,
                    json!({
                        "type": "tool_finished",
                        "tool": "take_screenshot",
                        "success": success,
                    }),
                );
                let _ = self.events.send(AgentEvent::ToolFinished {
                    task_id,
                    tool: "take_screenshot".into(),
                });
                if !success {
                    return Err(text);
                }
                self.note_private_tool_output("take_screenshot", &input, &text);
                let image = image.ok_or_else(|| "screenshot was empty".to_string())?;
                Ok(ImagePart {
                    media_type: image.media_type,
                    bytes: image.bytes,
                    width: Some(image.width),
                    height: Some(image.height),
                })
            }
        }
    }

    pub(crate) fn persist_step_progress(
        &self,
        task_dir: &Path,
        reasoning: &str,
        reasoning_ms: Option<u64>,
        assistant_text: &str,
    ) {
        if !reasoning.trim().is_empty() {
            append_event_log(
                task_dir,
                json!({
                    "type": "reasoning",
                    "text": redact_known_values(reasoning, &self.private_values),
                    "duration_ms": reasoning_ms,
                }),
            );
        }
        let trimmed = assistant_text.trim_start();
        if !trimmed.is_empty() {
            append_event_log(
                task_dir,
                json!({
                    "type": "assistant_text",
                    "text": redact_known_values(trimmed, &self.private_values),
                }),
            );
        }
    }

    pub(crate) async fn execute_tool(
        &mut self,
        task_id: TaskId,
        name: String,
        context: ToolContext,
        input: serde_json::Value,
    ) -> ToolExec {
        let cancel = Arc::clone(&context.cancel);
        let tools = Arc::clone(&self.tools);
        let mut handle = tokio::task::spawn_blocking(move || tools.execute(&name, &context, input));
        let timeout = tokio::time::sleep(DEFAULT_TOOL_TIMEOUT);
        tokio::pin!(timeout);
        loop {
            tokio::select! {
                biased;
                result = &mut handle => {
                    return match result {
                        Ok(Ok(result)) => ToolExec::Done {
                            text: result.text,
                            created: result.created_file,
                            image: result.image,
                            success: true,
                        },
                        Ok(Err(err)) => ToolExec::Done {
                            text: err.to_string(),
                            created: None,
                            image: None,
                            success: false,
                        },
                        Err(_) => ToolExec::Done {
                            text: "tool failed".into(),
                            created: None,
                            image: None,
                            success: false,
                        },
                    };
                }
                _ = &mut timeout => {
                    cancel.store(true, Ordering::SeqCst);
                    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
                    return ToolExec::Done {
                        text: "tool timed out".into(),
                        created: None,
                        image: None,
                        success: false,
                    };
                }
                cmd = self.commands.recv() => {
                    match cmd {
                        None => {
                            cancel.store(true, Ordering::SeqCst);
                            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
                            return ToolExec::Cancelled { reset: false };
                        }
                        Some(RuntimeCommand::Cancel(id)) if id == task_id => {
                            cancel.store(true, Ordering::SeqCst);
                            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
                            return ToolExec::Cancelled { reset: false };
                        }
                        Some(RuntimeCommand::ResetSession) => {
                            cancel.store(true, Ordering::SeqCst);
                            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
                            return ToolExec::Cancelled { reset: true };
                        }
                        Some(RuntimeCommand::TestConnection) => self.spawn_test_connection(),
                        Some(RuntimeCommand::TestCompat { id }) => {
                            self.spawn_test_connection_for(Some(id));
                        }
                        Some(other) => self.deferred_commands.push_back(other),
                    }
                }
            }
        }
    }
}
