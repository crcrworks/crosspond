use std::path::Path;

use crosspond_knowledge::{ActivityStatus, KnowledgeBrief};
use serde_json::json;

use crate::command::RuntimeCommand;
use crate::conversation::write_session_redacted;
use crate::event::AgentEvent;
use crate::ids::TaskId;
use crate::receipt::append_event_log;

use super::Runtime;

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish_cancelled(
        &mut self,
        task_id: TaskId,
        prompt: &str,
        task_dir: &Path,
        reset: bool,
        reused_scratch: bool,
        artifacts: &[String],
        brief: Option<&KnowledgeBrief>,
        actions: &[String],
    ) {
        let path = self.finish_scratch(reused_scratch, artifacts, reset);
        self.write_meta(task_dir, task_id, prompt, "cancelled", path.as_deref());
        append_event_log(task_dir, json!({ "type": "task_cancelled" }));
        write_session_redacted(task_dir, &self.session, &self.private_values);
        self.record_activity(
            brief,
            prompt,
            ActivityStatus::Cancelled,
            "",
            actions,
            artifacts,
        );
        let _ = self.events.send(AgentEvent::TaskCancelled { task_id });
    }

    pub(crate) fn drain_control(&mut self, task_id: TaskId) -> Option<bool> {
        let mut reset = false;
        let mut cancelled = false;
        loop {
            match self.commands.try_recv() {
                Ok(RuntimeCommand::Cancel(id)) if id == task_id => cancelled = true,
                Ok(RuntimeCommand::ResetSession) => {
                    cancelled = true;
                    reset = true;
                }
                Ok(RuntimeCommand::TestConnection) => self.spawn_test_connection(),
                Ok(RuntimeCommand::TestCompat { id }) => self.spawn_test_connection_for(Some(id)),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        cancelled.then_some(reset)
    }
}
