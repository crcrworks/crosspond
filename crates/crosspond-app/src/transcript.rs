//! Collapsible work groups for the command window transcript.
//!
//! While a turn is still working, steps render inline. The first assistant
//! text seals the group under a collapsed "Worked for …" header so the final
//! answer sits below a tidy summary. If more tools follow, that text is
//! absorbed as narration and the group opens inline again.

use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transcript {
    blocks: Vec<TranscriptBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranscriptBlock {
    User {
        text: String,
    },
    Work {
        steps: Vec<WorkStep>,
        expanded: bool,
        started_at: Instant,
        /// `Some` once sealed; header becomes "Worked for …".
        worked: Option<Duration>,
    },
    Text {
        text: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkStep {
    Thinking {
        text: String,
        expanded: bool,
        started_at: Instant,
        duration: Option<Duration>,
    },
    Narration {
        text: String,
    },
    Tool(ToolLine),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolLine {
    pub name: String,
    pub summary: String,
    pub running: bool,
}

impl Transcript {
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    pub fn blocks(&self) -> &[TranscriptBlock] {
        &self.blocks
    }

    pub fn clear(&mut self) {
        self.blocks.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Whether the current turn (after the latest user message) already has assistant text.
    pub fn has_assistant_text_since_last_user(&self) -> bool {
        for block in self.blocks.iter().rev() {
            match block {
                TranscriptBlock::User { .. } => return false,
                TranscriptBlock::Text { text } if !text.trim().is_empty() => return true,
                _ => {}
            }
        }
        false
    }

    /// Append a user turn. Never merges with assistant text; seals open work.
    pub fn push_user(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        self.seal_open_work();
        self.blocks.push(TranscriptBlock::User {
            text: trimmed.to_string(),
        });
    }

    pub fn push_reasoning(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.absorb_trailing_text();
        if let Some(idx) = self.reopen_work()
            && let TranscriptBlock::Work { steps, .. } = &mut self.blocks[idx]
        {
            if let Some(WorkStep::Thinking {
                text,
                duration: None,
                ..
            }) = steps.last_mut()
            {
                text.push_str(delta);
                return;
            }
            if delta.trim().is_empty() {
                return;
            }
            freeze_thinking_steps(steps);
            steps.push(thinking_step(delta.to_string()));
            return;
        }
        if delta.trim().is_empty() {
            return;
        }
        self.blocks.push(TranscriptBlock::Work {
            steps: vec![thinking_step(delta.to_string())],
            expanded: true,
            started_at: Instant::now(),
            worked: None,
        });
    }

    pub fn push_text(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.freeze_open_thinking();
        if let Some(TranscriptBlock::Text { text }) = self.blocks.last_mut() {
            text.push_str(delta);
            return;
        }
        let trimmed = delta.trim_start();
        if trimmed.is_empty() {
            return;
        }
        self.seal_open_work();
        self.blocks.push(TranscriptBlock::Text {
            text: trimmed.to_string(),
        });
    }

    pub fn push_notice(&mut self, message: &str) {
        if message.is_empty() {
            return;
        }
        self.seal_open_work();
        self.blocks.push(TranscriptBlock::Text {
            text: message.to_string(),
        });
    }

    pub fn start_tool(&mut self, name: &str, summary: &str) {
        self.absorb_trailing_text();
        self.freeze_open_thinking();
        let line = ToolLine {
            name: name.to_string(),
            summary: summary.to_string(),
            running: true,
        };
        if let Some(idx) = self.reopen_work()
            && let TranscriptBlock::Work { steps, .. } = &mut self.blocks[idx]
        {
            steps.push(WorkStep::Tool(line));
            return;
        }
        self.blocks.push(TranscriptBlock::Work {
            steps: vec![WorkStep::Tool(line)],
            expanded: true,
            started_at: Instant::now(),
            worked: None,
        });
    }

    pub fn finish_tool(&mut self, name: &str) {
        let Some(idx) = self.turn_work_index() else {
            return;
        };
        let TranscriptBlock::Work { steps, .. } = &mut self.blocks[idx] else {
            return;
        };
        if let Some(item) = steps.iter_mut().rev().find_map(|step| match step {
            WorkStep::Tool(item) if item.running && item.name == name => Some(item),
            _ => None,
        }) {
            item.running = false;
            return;
        }
        if let Some(item) = steps.iter_mut().rev().find_map(|step| match step {
            WorkStep::Tool(item) if item.running => Some(item),
            _ => None,
        }) {
            item.running = false;
        }
    }

    /// Stop shimmer animations after the task ends, even if a ToolFinished was missed.
    pub fn finish_running_tools(&mut self) {
        for block in &mut self.blocks {
            if let TranscriptBlock::Work { steps, .. } = block {
                for step in steps {
                    if let WorkStep::Tool(item) = step {
                        item.running = false;
                    }
                }
            }
        }
        self.seal_open_work();
    }

    /// Seal the open work group so its header becomes "Worked for …".
    pub fn seal_open_work(&mut self) {
        self.freeze_open_thinking();
        if let Some(idx) = self.open_work_index()
            && let TranscriptBlock::Work {
                expanded,
                started_at,
                worked,
                ..
            } = &mut self.blocks[idx]
        {
            *worked = Some(started_at.elapsed());
            *expanded = false;
        }
    }

    fn absorb_trailing_text(&mut self) {
        let Some(TranscriptBlock::Text { text }) = self.blocks.last() else {
            return;
        };
        if text.trim().is_empty() {
            self.blocks.pop();
            return;
        }
        let text = match self.blocks.pop() {
            Some(TranscriptBlock::Text { text }) => text,
            Some(other) => {
                self.blocks.push(other);
                return;
            }
            None => return,
        };
        if let Some(idx) = self.reopen_work()
            && let TranscriptBlock::Work { steps, .. } = &mut self.blocks[idx]
        {
            steps.push(WorkStep::Narration { text });
            return;
        }
        self.blocks.push(TranscriptBlock::Work {
            steps: vec![WorkStep::Narration { text }],
            expanded: true,
            started_at: Instant::now(),
            worked: None,
        });
    }

    /// Work in the current turn, including a group already sealed by assistant text.
    fn turn_work_index(&self) -> Option<usize> {
        for (idx, block) in self.blocks.iter().enumerate().rev() {
            match block {
                TranscriptBlock::Text { .. } => continue,
                TranscriptBlock::User { .. } => return None,
                TranscriptBlock::Work { .. } => return Some(idx),
            }
        }
        None
    }

    fn reopen_work(&mut self) -> Option<usize> {
        let idx = self.turn_work_index()?;
        if let TranscriptBlock::Work {
            worked, expanded, ..
        } = &mut self.blocks[idx]
        {
            *worked = None;
            *expanded = true;
        }
        Some(idx)
    }

    fn freeze_open_thinking(&mut self) {
        if let Some(idx) = self.open_work_index()
            && let TranscriptBlock::Work { steps, .. } = &mut self.blocks[idx]
        {
            freeze_thinking_steps(steps);
        }
    }

    fn open_work_index(&self) -> Option<usize> {
        for (idx, block) in self.blocks.iter().enumerate().rev() {
            match block {
                TranscriptBlock::Text { .. } => continue,
                TranscriptBlock::User { .. } => return None,
                TranscriptBlock::Work { worked: None, .. } => return Some(idx),
                _ => return None,
            }
        }
        None
    }

    /// Index of the open work block whose latest step is still thinking (for live shimmer).
    pub fn live_thinking_index(&self) -> Option<usize> {
        let idx = self.open_work_index()?;
        let TranscriptBlock::Work { steps, .. } = &self.blocks[idx] else {
            return None;
        };
        match steps.last() {
            Some(WorkStep::Thinking { duration: None, .. }) => Some(idx),
            _ => None,
        }
    }

    pub fn toggle(&mut self, index: usize) {
        if let Some(TranscriptBlock::Work { expanded, .. }) = self.blocks.get_mut(index) {
            *expanded = !*expanded;
        }
    }

    pub fn toggle_step(&mut self, block: usize, step: usize) {
        if let Some(TranscriptBlock::Work { steps, .. }) = self.blocks.get_mut(block)
            && let Some(WorkStep::Thinking { expanded, .. }) = steps.get_mut(step)
        {
            *expanded = !*expanded;
        }
    }

    #[cfg(test)]
    pub fn live_activity(&self) -> LiveActivity {
        if let Some(name) = self.running_tool() {
            return LiveActivity::Tool(name.to_string());
        }
        for block in self.blocks.iter().rev() {
            match block {
                TranscriptBlock::Work { steps, worked, .. } => {
                    if worked.is_some() {
                        continue;
                    }
                    return match steps.last() {
                        Some(WorkStep::Thinking { duration: None, .. }) => LiveActivity::Thinking,
                        Some(WorkStep::Thinking { .. }) | Some(WorkStep::Narration { .. }) => {
                            LiveActivity::Writing
                        }
                        Some(WorkStep::Tool(_)) => LiveActivity::PreparingNextMoves,
                        None => LiveActivity::Thinking,
                    };
                }
                TranscriptBlock::Text { text } if text.trim().is_empty() => continue,
                TranscriptBlock::Text { .. } => return LiveActivity::Writing,
                TranscriptBlock::User { .. } => return LiveActivity::Thinking,
            }
        }
        LiveActivity::Thinking
    }

    pub fn running_tool(&self) -> Option<&str> {
        for block in self.blocks.iter().rev() {
            if let TranscriptBlock::Work {
                steps,
                worked: None,
                ..
            } = block
                && let Some(current) = steps.iter().rev().find_map(|step| match step {
                    WorkStep::Tool(item) if item.running => Some(item.name.as_str()),
                    _ => None,
                })
            {
                return Some(current);
            }
        }
        None
    }
}

impl Default for Transcript {
    fn default() -> Self {
        Self::new()
    }
}

/// What the agent is doing right now, for the Cursor-style footer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LiveActivity {
    Thinking,
    PreparingNextMoves,
    Writing,
    Tool(String),
}

impl LiveActivity {
    pub fn label(&self) -> String {
        match self {
            Self::Thinking => "Thinking".into(),
            Self::PreparingNextMoves => "Preparing next moves".into(),
            Self::Writing => "Writing".into(),
            Self::Tool(name) => tool_activity_label(name).trim_end_matches('…').to_string(),
        }
    }
}

impl TranscriptBlock {
    pub fn collapsed_label(&self, thinking_live: bool) -> String {
        match self {
            Self::Work {
                worked: Some(duration),
                ..
            } => worked_for_label(*duration),
            Self::Work {
                steps,
                worked: None,
                ..
            } => live_work_label(steps, thinking_live),
            Self::User { .. } | Self::Text { .. } => String::new(),
        }
    }
}

pub fn worked_for_label(duration: Duration) -> String {
    format!("Worked for {}", compact_duration(duration))
}

pub fn compact_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs.max(1))
    } else {
        let mins = secs / 60;
        let rem = secs % 60;
        if rem == 0 {
            format!("{mins}m")
        } else {
            format!("{mins}m {rem}s")
        }
    }
}

pub fn thought_label(duration: Option<Duration>, started_at: Instant, live: bool) -> String {
    if live {
        return "Thinking".into();
    }
    let elapsed = duration.unwrap_or_else(|| started_at.elapsed());
    format!("Thought {}", compact_duration(elapsed))
}

fn thinking_step(text: String) -> WorkStep {
    WorkStep::Thinking {
        text,
        expanded: false,
        started_at: Instant::now(),
        duration: None,
    }
}

fn freeze_thinking_steps(steps: &mut [WorkStep]) {
    if let Some(WorkStep::Thinking {
        started_at,
        duration,
        ..
    }) = steps.last_mut()
        && duration.is_none()
    {
        *duration = Some(started_at.elapsed());
    }
}

fn live_work_label(steps: &[WorkStep], thinking_live: bool) -> String {
    let tools: Vec<&ToolLine> = steps
        .iter()
        .filter_map(|step| match step {
            WorkStep::Tool(item) => Some(item),
            _ => None,
        })
        .collect();
    if let Some(current) = tools.iter().rev().find(|item| item.running) {
        return tool_activity_label(&current.name);
    }
    if thinking_live || tools.is_empty() {
        let has_thinking = steps
            .iter()
            .any(|step| matches!(step, WorkStep::Thinking { text, .. } if !text.trim().is_empty()));
        return if has_thinking && !thinking_live {
            "Thought".into()
        } else {
            "Thinking".into()
        };
    }
    match tools.len() {
        0 => "Thinking".into(),
        1 => tool_done_label(&tools[0].name),
        n => format!("Used {n} tools"),
    }
}

pub fn tool_activity_label(name: &str) -> String {
    match name {
        "read_file" => "Reading file…".into(),
        "write_file" => "Writing file…".into(),
        "list_directory" => "Listing directory…".into(),
        "create_directory" => "Creating directory…".into(),
        "list_apps" => "Listing apps…".into(),
        "open_app" => "Opening an app…".into(),
        "focus_app" => "Focusing an app…".into(),
        "get_accessibility_snapshot" => "Looking at the screen…".into(),
        "take_screenshot" => "Taking a screenshot…".into(),
        "ui_press" => "Pressing a control…".into(),
        "ui_set_value" => "Filling a field…".into(),
        "ui_click" => "Clicking…".into(),
        "ui_type" => "Typing…".into(),
        "ui_hotkey" => "Sending a shortcut…".into(),
        "ui_scroll" => "Scrolling…".into(),
        "calendar_events" => "Reading the calendar…".into(),
        "knowledge_search" => "Searching vault…".into(),
        "knowledge_find_procedure" => "Finding a procedure…".into(),
        "knowledge_read" => "Reading a note…".into(),
        "knowledge_neighbors" => "Following note links…".into(),
        "knowledge_backlinks" => "Finding backlinks…".into(),
        "knowledge_ingest" => "Ingesting into vault…".into(),
        "knowledge_propose_update" => "Proposing a vault update…".into(),
        "knowledge_read_later" => "Saving for later…".into(),
        "knowledge_archive_source" => "Archiving a source…".into(),
        "run_command" => "Running a command…".into(),
        "open_url" => "Opening a URL…".into(),
        "web_search" => "Searching the web…".into(),
        "fetch_url" => "Fetching a page…".into(),
        other => format!("Running {other}…"),
    }
}

pub fn tool_done_label(name: &str) -> String {
    match name {
        "read_file" => "Read a file".into(),
        "write_file" => "Wrote a file".into(),
        "list_directory" => "Listed a directory".into(),
        "create_directory" => "Created a directory".into(),
        "list_apps" => "Listed apps".into(),
        "open_app" => "Opened an app".into(),
        "focus_app" => "Focused an app".into(),
        "get_accessibility_snapshot" => "Looked at the screen".into(),
        "take_screenshot" => "Took a screenshot".into(),
        "ui_press" => "Pressed a control".into(),
        "ui_set_value" => "Filled a field".into(),
        "ui_click" => "Clicked".into(),
        "ui_type" => "Typed".into(),
        "ui_hotkey" => "Sent a shortcut".into(),
        "ui_scroll" => "Scrolled".into(),
        "calendar_events" => "Read the calendar".into(),
        "knowledge_search" => "Searched vault".into(),
        "knowledge_find_procedure" => "Found a procedure".into(),
        "knowledge_read" => "Read a note".into(),
        "knowledge_neighbors" => "Followed note links".into(),
        "knowledge_backlinks" => "Found backlinks".into(),
        "knowledge_ingest" => "Ingested into vault".into(),
        "knowledge_propose_update" => "Proposed a vault update".into(),
        "knowledge_read_later" => "Saved for later".into(),
        "knowledge_archive_source" => "Archived a source".into(),
        "run_command" => "Ran a command".into(),
        "open_url" => "Opened a URL".into(),
        "web_search" => "Searched the web".into(),
        "fetch_url" => "Fetched a page".into(),
        other => format!("Ran {other}"),
    }
}

/// Tool name plus argument summary, e.g. `knowledge_search  cursor origin`.
///
/// Uses the real tool id so vault search is not collapsed into the same
/// "Searched" label as web search.
pub fn tool_row_label(name: &str, summary: &str) -> String {
    let summary = summary.trim();
    if summary.is_empty() {
        name.to_string()
    } else {
        format!("{name}  {summary}")
    }
}

pub fn tool_icon_path(name: &str) -> &'static str {
    match name {
        "read_file" => "icons/file.svg",
        "write_file" => "icons/pencil.svg",
        "list_directory" | "create_directory" => "icons/folder.svg",
        "list_apps" | "open_app" | "focus_app" => "icons/monitor.svg",
        "get_accessibility_snapshot" | "take_screenshot" => "icons/monitor.svg",
        "ui_press" | "ui_click" | "ui_type" | "ui_hotkey" | "ui_scroll" => "icons/pointer.svg",
        "ui_set_value" => "icons/text.svg",
        "calendar_events" => "icons/file.svg",
        "knowledge_search" | "knowledge_find_procedure" => "icons/search.svg",
        "knowledge_read" | "knowledge_neighbors" | "knowledge_backlinks" => "icons/file.svg",
        "knowledge_ingest"
        | "knowledge_propose_update"
        | "knowledge_read_later"
        | "knowledge_archive_source" => "icons/pencil.svg",
        "run_command" | "open_url" => "icons/wrench.svg",
        "web_search" | "fetch_url" => "icons/search.svg",
        _ => "icons/wrench.svg",
    }
}

pub fn work_header_icon(steps: &[WorkStep]) -> Option<&'static str> {
    let tools: Vec<&ToolLine> = steps
        .iter()
        .filter_map(|step| match step {
            WorkStep::Tool(item) => Some(item),
            _ => None,
        })
        .collect();
    if let Some(current) = tools.iter().rev().find(|item| item.running) {
        return Some(tool_icon_path(&current.name));
    }
    match tools.len() {
        0 => None,
        1 => Some(tool_icon_path(&tools[0].name)),
        _ => Some("icons/wrench.svg"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(transcript: &mut Transcript, name: &str) {
        transcript.start_tool(name, "");
    }

    #[test]
    fn consecutive_tools_and_thinking_share_one_group() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("plan");
        start(&mut transcript, "get_accessibility_snapshot");
        transcript.finish_tool("get_accessibility_snapshot");
        start(&mut transcript, "ui_press");
        transcript.finish_tool("ui_press");
        transcript.push_text("Done.");
        start(&mut transcript, "read_file");
        assert_eq!(transcript.blocks().len(), 1);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                steps,
                expanded,
                worked: None,
                ..
            } => {
                assert_eq!(steps.len(), 5);
                assert!(*expanded);
                assert!(matches!(
                    &steps[0],
                    WorkStep::Thinking { text, .. } if text == "plan"
                ));
                assert!(matches!(
                    &steps[1],
                    WorkStep::Tool(ToolLine { name, running, .. })
                        if name == "get_accessibility_snapshot" && !*running
                ));
                assert!(matches!(
                    &steps[2],
                    WorkStep::Tool(ToolLine { name, running, .. })
                        if name == "ui_press" && !*running
                ));
                assert!(matches!(
                    &steps[3],
                    WorkStep::Narration { text } if text == "Done."
                ));
                assert!(matches!(
                    &steps[4],
                    WorkStep::Tool(ToolLine { name, running, .. })
                        if name == "read_file" && *running
                ));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn assistant_text_collapses_work_before_the_answer() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("plan");
        start(&mut transcript, "read_file");
        transcript.finish_tool("read_file");
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                worked: None,
                expanded,
                ..
            } => {
                assert!(*expanded);
            }
            other => panic!("{other:?}"),
        }
        transcript.push_text("What was done.");
        assert_eq!(transcript.blocks().len(), 2);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                expanded,
                worked: Some(_),
                ..
            } => {
                assert!(!*expanded);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            &transcript.blocks()[1],
            TranscriptBlock::Text { text } if text == "What was done."
        ));
        start(&mut transcript, "run_command");
        assert_eq!(transcript.blocks().len(), 1);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                steps,
                worked: None,
                expanded,
                ..
            } => {
                assert!(*expanded);
                assert!(matches!(&steps[2], WorkStep::Narration { .. }));
                assert!(matches!(
                    &steps[3],
                    WorkStep::Tool(ToolLine { name, .. }) if name == "run_command"
                ));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn final_text_stays_outside_when_work_seals() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("plan");
        start(&mut transcript, "read_file");
        transcript.finish_tool("read_file");
        transcript.push_text("What was done.");
        transcript.finish_running_tools();
        assert_eq!(transcript.blocks().len(), 2);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                steps,
                expanded,
                worked: Some(_),
                ..
            } => {
                assert!(!*expanded);
                assert_eq!(steps.len(), 2);
                assert!(matches!(&steps[0], WorkStep::Thinking { .. }));
                assert!(matches!(&steps[1], WorkStep::Tool(_)));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            &transcript.blocks()[1],
            TranscriptBlock::Text { text } if text == "What was done."
        ));
    }

    #[test]
    fn intermediate_text_is_absorbed_when_more_tools_run() {
        let mut transcript = Transcript::new();
        transcript.push_text("I'll help.");
        start(&mut transcript, "list_directory");
        transcript.finish_tool("list_directory");
        transcript.push_text("I found 5.");
        start(&mut transcript, "run_command");
        transcript.finish_tool("run_command");
        transcript.push_text("What was done.");
        transcript.finish_running_tools();
        assert_eq!(transcript.blocks().len(), 2);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                steps,
                worked: Some(_),
                ..
            } => {
                assert_eq!(steps.len(), 4);
                assert!(matches!(
                    &steps[0],
                    WorkStep::Narration { text } if text == "I'll help."
                ));
                assert!(matches!(
                    &steps[1],
                    WorkStep::Tool(ToolLine { name, .. }) if name == "list_directory"
                ));
                assert!(matches!(
                    &steps[2],
                    WorkStep::Narration { text } if text == "I found 5."
                ));
                assert!(matches!(
                    &steps[3],
                    WorkStep::Tool(ToolLine { name, .. }) if name == "run_command"
                ));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            &transcript.blocks()[1],
            TranscriptBlock::Text { text } if text == "What was done."
        ));
    }

    #[test]
    fn user_turn_does_not_merge_with_assistant_text() {
        let mut transcript = Transcript::new();
        transcript.push_user("hello");
        transcript.push_text("Hi there.");
        transcript.push_user("again");
        transcript.push_text("Sure.");
        assert_eq!(transcript.blocks().len(), 4);
        assert!(matches!(
            &transcript.blocks()[0],
            TranscriptBlock::User { text } if text == "hello"
        ));
        assert!(matches!(
            &transcript.blocks()[1],
            TranscriptBlock::Text { text } if text == "Hi there."
        ));
        assert!(matches!(
            &transcript.blocks()[2],
            TranscriptBlock::User { text } if text == "again"
        ));
        assert!(matches!(
            &transcript.blocks()[3],
            TranscriptBlock::Text { text } if text == "Sure."
        ));
    }

    #[test]
    fn user_turn_closes_work_groups() {
        let mut transcript = Transcript::new();
        transcript.push_user("first");
        transcript.push_reasoning("plan");
        start(&mut transcript, "read_file");
        transcript.finish_tool("read_file");
        transcript.push_text("Done.");
        transcript.push_user("follow-up");
        transcript.push_reasoning("next");
        start(&mut transcript, "ui_press");
        assert_eq!(transcript.blocks().len(), 5);
        match &transcript.blocks()[1] {
            TranscriptBlock::Work {
                steps,
                worked: Some(_),
                ..
            } => {
                assert_eq!(steps.len(), 2);
                assert!(matches!(
                    &steps[1],
                    WorkStep::Tool(ToolLine { name, .. }) if name == "read_file"
                ));
            }
            other => panic!("{other:?}"),
        }
        match &transcript.blocks()[4] {
            TranscriptBlock::Work {
                steps,
                worked: None,
                ..
            } => {
                assert_eq!(steps.len(), 2);
                assert!(matches!(
                    &steps[0],
                    WorkStep::Thinking { text, .. } if text == "next"
                ));
                assert!(matches!(
                    &steps[1],
                    WorkStep::Tool(ToolLine { name, .. }) if name == "ui_press"
                ));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn follow_up_keeps_prior_user_and_assistant() {
        let mut transcript = Transcript::new();
        transcript.push_user("summarize this");
        transcript.push_text("Here is a summary.");
        assert!(transcript.has_assistant_text_since_last_user());
        transcript.push_user("make it shorter");
        assert_eq!(transcript.blocks().len(), 3);
        assert!(!transcript.is_empty());
        assert!(!transcript.has_assistant_text_since_last_user());
        assert!(matches!(
            &transcript.blocks()[0],
            TranscriptBlock::User { text } if text == "summarize this"
        ));
        assert!(matches!(
            &transcript.blocks()[1],
            TranscriptBlock::Text { text } if text == "Here is a summary."
        ));
        assert!(matches!(
            &transcript.blocks()[2],
            TranscriptBlock::User { text } if text == "make it shorter"
        ));
    }

    #[test]
    fn collapsed_work_shows_latest_running_then_summary() {
        let mut transcript = Transcript::new();
        start(&mut transcript, "get_accessibility_snapshot");
        assert_eq!(
            transcript.blocks()[0].collapsed_label(false),
            "Looking at the screen…"
        );
        transcript.finish_tool("get_accessibility_snapshot");
        start(&mut transcript, "ui_press");
        assert_eq!(
            transcript.blocks()[0].collapsed_label(false),
            "Pressing a control…"
        );
        transcript.finish_tool("ui_press");
        assert_eq!(
            transcript.blocks()[0].collapsed_label(false),
            "Used 2 tools"
        );
    }

    #[test]
    fn thinking_and_tools_share_one_work_group() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("hmm");
        start(&mut transcript, "read_file");
        assert_eq!(transcript.blocks().len(), 1);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                steps, expanded, ..
            } => {
                assert_eq!(steps.len(), 2);
                assert!(*expanded);
            }
            other => panic!("{other:?}"),
        }
        transcript.toggle(0);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                expanded, steps, ..
            } => {
                assert!(!*expanded);
                assert!(matches!(
                    &steps[0],
                    WorkStep::Thinking { text, .. } if text == "hmm"
                ));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn finish_running_tools_clears_and_seals() {
        let mut transcript = Transcript::new();
        start(&mut transcript, "read_file");
        start(&mut transcript, "ui_press");
        assert!(transcript.running_tool().is_some());
        transcript.finish_running_tools();
        assert!(transcript.running_tool().is_none());
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                steps,
                worked: Some(_),
                expanded,
                ..
            } => {
                assert!(!*expanded);
                assert!(steps.iter().all(|step| match step {
                    WorkStep::Tool(item) => !item.running,
                    _ => true,
                }));
                assert!(
                    transcript.blocks()[0]
                        .collapsed_label(false)
                        .starts_with("Worked for ")
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn thinking_between_tools_stays_in_the_same_group() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("first");
        start(&mut transcript, "get_accessibility_snapshot");
        transcript.finish_tool("get_accessibility_snapshot");
        transcript.push_reasoning(" more");
        start(&mut transcript, "ui_press");
        transcript.finish_tool("ui_press");
        assert_eq!(transcript.blocks().len(), 1);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work { steps, .. } => {
                assert_eq!(steps.len(), 4);
                assert!(matches!(
                    &steps[0],
                    WorkStep::Thinking { text, duration: Some(_), .. } if text == "first"
                ));
                assert!(matches!(
                    &steps[1],
                    WorkStep::Tool(ToolLine { name, .. })
                        if name == "get_accessibility_snapshot"
                ));
                assert!(matches!(
                    &steps[2],
                    WorkStep::Thinking { text, duration: Some(_), .. } if text == " more"
                ));
                assert!(matches!(
                    &steps[3],
                    WorkStep::Tool(ToolLine { name, .. }) if name == "ui_press"
                ));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(transcript.live_thinking_index(), None);
        transcript.push_reasoning("next");
        assert_eq!(transcript.live_thinking_index(), Some(0));
        match &transcript.blocks()[0] {
            TranscriptBlock::Work { steps, .. } => {
                assert_eq!(steps.len(), 5);
                assert!(matches!(
                    &steps[4],
                    WorkStep::Thinking { text, duration: None, .. } if text == "next"
                ));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn whitespace_only_text_does_not_start_a_block() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("plan");
        transcript.push_text("\n\n\n");
        start(&mut transcript, "ui_press");
        assert_eq!(transcript.blocks().len(), 1);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work {
                steps,
                worked: None,
                ..
            } => {
                assert_eq!(steps.len(), 2);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn sealed_work_uses_worked_for_label() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("plan");
        start(&mut transcript, "read_file");
        transcript.finish_tool("read_file");
        transcript.push_text("Done.");
        transcript.finish_running_tools();
        let label = transcript.blocks()[0].collapsed_label(false);
        assert!(label.starts_with("Worked for "));
        assert!(label.ends_with('s') || label.contains('m'));
    }

    #[test]
    fn worked_for_label_formats_seconds_and_minutes() {
        assert_eq!(worked_for_label(Duration::from_secs(0)), "Worked for 1s");
        assert_eq!(worked_for_label(Duration::from_secs(1)), "Worked for 1s");
        assert_eq!(worked_for_label(Duration::from_secs(12)), "Worked for 12s");
        assert_eq!(worked_for_label(Duration::from_secs(60)), "Worked for 1m");
        assert_eq!(
            worked_for_label(Duration::from_secs(179)),
            "Worked for 2m 59s"
        );
        assert_eq!(worked_for_label(Duration::from_secs(180)), "Worked for 3m");
    }

    #[test]
    fn thought_label_is_thinking_while_live_then_duration() {
        let started = Instant::now();
        assert_eq!(
            thought_label(Some(Duration::from_secs(2)), started, true),
            "Thinking"
        );
        assert_eq!(
            thought_label(Some(Duration::from_secs(2)), started, false),
            "Thought 2s"
        );
        assert_eq!(
            thought_label(Some(Duration::from_secs(75)), started, false),
            "Thought 1m 15s"
        );
        assert!(thought_label(None, started, false).starts_with("Thought "));
    }

    #[test]
    fn tool_row_label_uses_the_tool_name() {
        assert_eq!(
            tool_row_label("knowledge_search", "cursor origin"),
            "knowledge_search  cursor origin"
        );
        assert_eq!(
            tool_row_label("web_search", "cursor origin"),
            "web_search  cursor origin"
        );
        assert_eq!(
            tool_row_label("knowledge_read", "cp_01a010b7ddd07872a2694132d636125b"),
            "knowledge_read  cp_01a010b7ddd07872a2694132d636125b"
        );
        assert_eq!(
            tool_row_label(
                "fetch_url",
                "https://cursor.com/changelog/origin-code-hosting"
            ),
            "fetch_url  https://cursor.com/changelog/origin-code-hosting"
        );
        assert_eq!(
            tool_row_label("run_command", "ls -la"),
            "run_command  ls -la"
        );
        assert_eq!(tool_row_label("ui_type", ""), "ui_type");
    }

    #[test]
    fn tool_icon_matches_known_tools() {
        assert_eq!(tool_icon_path("read_file"), "icons/file.svg");
        assert_eq!(
            tool_icon_path("get_accessibility_snapshot"),
            "icons/monitor.svg"
        );
        assert_eq!(tool_icon_path("take_screenshot"), "icons/monitor.svg");
        assert_eq!(tool_icon_path("ui_click"), "icons/pointer.svg");
        assert_eq!(tool_icon_path("ui_press"), "icons/pointer.svg");
        assert_eq!(tool_icon_path("web_search"), "icons/search.svg");
        assert_eq!(tool_icon_path("fetch_url"), "icons/search.svg");
        assert_eq!(tool_icon_path("list_apps"), "icons/monitor.svg");
        assert_eq!(tool_icon_path("calendar_events"), "icons/file.svg");
        assert_eq!(tool_icon_path("knowledge_search"), "icons/search.svg");
        assert_eq!(tool_icon_path("knowledge_read"), "icons/file.svg");
        assert_eq!(tool_icon_path("ui_type"), "icons/pointer.svg");
        assert_eq!(tool_icon_path("run_command"), "icons/wrench.svg");
        assert_eq!(tool_icon_path("unknown_tool"), "icons/wrench.svg");
    }

    #[test]
    fn live_activity_follows_the_agent_phase() {
        let mut transcript = Transcript::new();
        assert_eq!(transcript.live_activity(), LiveActivity::Thinking);
        assert_eq!(transcript.live_activity().label(), "Thinking");

        transcript.push_reasoning("plan");
        assert_eq!(transcript.live_activity(), LiveActivity::Thinking);

        start(&mut transcript, "get_accessibility_snapshot");
        assert_eq!(
            transcript.live_activity(),
            LiveActivity::Tool("get_accessibility_snapshot".into())
        );
        assert_eq!(transcript.live_activity().label(), "Looking at the screen");

        transcript.finish_tool("get_accessibility_snapshot");
        assert_eq!(transcript.live_activity(), LiveActivity::PreparingNextMoves);
        assert_eq!(transcript.live_activity().label(), "Preparing next moves");

        transcript.push_text("Done.");
        assert_eq!(transcript.live_activity(), LiveActivity::Writing);
        assert_eq!(transcript.live_activity().label(), "Writing");
    }

    #[test]
    fn header_icon_follows_the_latest_running_tool() {
        let mut transcript = Transcript::new();
        start(&mut transcript, "get_accessibility_snapshot");
        match &transcript.blocks()[0] {
            TranscriptBlock::Work { steps, .. } => {
                assert_eq!(work_header_icon(steps), Some("icons/monitor.svg"));
            }
            other => panic!("{other:?}"),
        }
        transcript.finish_tool("get_accessibility_snapshot");
        start(&mut transcript, "ui_press");
        match &transcript.blocks()[0] {
            TranscriptBlock::Work { steps, .. } => {
                assert_eq!(work_header_icon(steps), Some("icons/pointer.svg"));
            }
            other => panic!("{other:?}"),
        }
        transcript.finish_tool("ui_press");
        match &transcript.blocks()[0] {
            TranscriptBlock::Work { steps, .. } => {
                assert_eq!(work_header_icon(steps), Some("icons/wrench.svg"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn toggle_step_expands_nested_thinking() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("plan");
        start(&mut transcript, "read_file");
        transcript.toggle_step(0, 0);
        match &transcript.blocks()[0] {
            TranscriptBlock::Work { steps, .. } => match &steps[0] {
                WorkStep::Thinking { expanded, text, .. } => {
                    assert!(*expanded);
                    assert_eq!(text, "plan");
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }
}
