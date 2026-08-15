//! Collapsible thinking / tool groups for the command window transcript.
//!
//! Matches the Cursor / Codex pattern: consecutive tools collapse into one
//! header; visible assistant text starts a new group below it.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transcript {
    blocks: Vec<TranscriptBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranscriptBlock {
    Thinking {
        text: String,
        expanded: bool,
    },
    Tools {
        items: Vec<ToolLine>,
        expanded: bool,
    },
    Text {
        text: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolLine {
    pub name: String,
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

    pub fn has_assistant_text(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| matches!(block, TranscriptBlock::Text { text } if !text.trim().is_empty()))
    }

    pub fn push_reasoning(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        if let Some(idx) = self.last_group_index(GroupKind::Thinking) {
            if let TranscriptBlock::Thinking { text, .. } = &mut self.blocks[idx] {
                text.push_str(delta);
            }
            return;
        }
        if delta.trim().is_empty() {
            return;
        }
        self.blocks.push(TranscriptBlock::Thinking {
            text: delta.to_string(),
            expanded: false,
        });
    }

    pub fn push_text(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        if let Some(TranscriptBlock::Text { text }) = self.blocks.last_mut() {
            text.push_str(delta);
            return;
        }
        let trimmed = delta.trim_start();
        if trimmed.is_empty() {
            return;
        }
        self.blocks.push(TranscriptBlock::Text {
            text: trimmed.to_string(),
        });
    }

    pub fn push_notice(&mut self, message: &str) {
        if message.is_empty() {
            return;
        }
        self.blocks.push(TranscriptBlock::Text {
            text: message.to_string(),
        });
    }

    pub fn start_tool(&mut self, name: &str) {
        let line = ToolLine {
            name: name.to_string(),
            running: true,
        };
        if let Some(idx) = self.last_group_index(GroupKind::Tools) {
            if let TranscriptBlock::Tools { items, .. } = &mut self.blocks[idx] {
                items.push(line);
            }
            return;
        }
        self.blocks.push(TranscriptBlock::Tools {
            items: vec![line],
            expanded: false,
        });
    }

    pub fn finish_tool(&mut self, name: &str) {
        let Some(idx) = self.last_group_index(GroupKind::Tools) else {
            return;
        };
        let TranscriptBlock::Tools { items, .. } = &mut self.blocks[idx] else {
            return;
        };
        if let Some(item) = items
            .iter_mut()
            .rev()
            .find(|item| item.running && item.name == name)
        {
            item.running = false;
            return;
        }
        if let Some(item) = items.iter_mut().rev().find(|item| item.running) {
            item.running = false;
        }
    }

    fn last_group_index(&self, kind: GroupKind) -> Option<usize> {
        for (idx, block) in self.blocks.iter().enumerate().rev() {
            match block {
                TranscriptBlock::Text { text } if text.trim().is_empty() => continue,
                TranscriptBlock::Text { .. } => return None,
                TranscriptBlock::Thinking { .. } if kind == GroupKind::Thinking => {
                    return Some(idx);
                }
                TranscriptBlock::Tools { .. } if kind == GroupKind::Tools => return Some(idx),
                TranscriptBlock::Thinking { .. } | TranscriptBlock::Tools { .. } => continue,
            }
        }
        None
    }

    pub fn toggle(&mut self, index: usize) {
        match self.blocks.get_mut(index) {
            Some(TranscriptBlock::Thinking { expanded, .. })
            | Some(TranscriptBlock::Tools { expanded, .. }) => {
                *expanded = !*expanded;
            }
            _ => {}
        }
    }

    #[cfg(test)]
    pub fn live_activity(&self) -> LiveActivity {
        if let Some(name) = self.running_tool() {
            return LiveActivity::Tool(name.to_string());
        }
        for block in self.blocks.iter().rev() {
            match block {
                TranscriptBlock::Tools { .. } => return LiveActivity::PreparingNextMoves,
                TranscriptBlock::Thinking { .. } => return LiveActivity::Thinking,
                TranscriptBlock::Text { text } if text.trim().is_empty() => continue,
                TranscriptBlock::Text { .. } => return LiveActivity::Writing,
            }
        }
        LiveActivity::Thinking
    }

    pub fn running_tool(&self) -> Option<&str> {
        for block in self.blocks.iter().rev() {
            if let TranscriptBlock::Tools { items, .. } = block
                && let Some(current) = items.iter().rev().find(|item| item.running)
            {
                return Some(current.name.as_str());
            }
        }
        None
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum GroupKind {
    Thinking,
    Tools,
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
    pub fn collapsed_label(&self) -> String {
        match self {
            Self::Thinking { text, .. } => {
                if text.is_empty() {
                    "Thinking…".into()
                } else {
                    "Thought".into()
                }
            }
            Self::Tools { items, .. } => collapsed_tools_label(items),
            Self::Text { .. } => String::new(),
        }
    }
}

pub fn tool_activity_label(name: &str) -> String {
    match name {
        "read_file" => "Reading file…".into(),
        "write_file" => "Writing file…".into(),
        "list_directory" => "Listing directory…".into(),
        "create_directory" => "Creating directory…".into(),
        "get_accessibility_snapshot" => "Looking at the screen…".into(),
        "ui_press" => "Pressing a control…".into(),
        "ui_set_value" => "Filling a field…".into(),
        other => format!("Running {other}…"),
    }
}

pub fn tool_done_label(name: &str) -> String {
    match name {
        "read_file" => "Read a file".into(),
        "write_file" => "Wrote a file".into(),
        "list_directory" => "Listed a directory".into(),
        "create_directory" => "Created a directory".into(),
        "get_accessibility_snapshot" => "Looked at the screen".into(),
        "ui_press" => "Pressed a control".into(),
        "ui_set_value" => "Filled a field".into(),
        other => format!("Ran {other}"),
    }
}

pub fn tool_icon_path(name: &str) -> &'static str {
    match name {
        "read_file" => "icons/file.svg",
        "write_file" => "icons/pencil.svg",
        "list_directory" | "create_directory" => "icons/folder.svg",
        "get_accessibility_snapshot" => "icons/monitor.svg",
        "ui_press" => "icons/pointer.svg",
        "ui_set_value" => "icons/text.svg",
        _ => "icons/wrench.svg",
    }
}

pub fn tools_header_icon(items: &[ToolLine]) -> &'static str {
    if let Some(current) = items.iter().rev().find(|item| item.running) {
        return tool_icon_path(&current.name);
    }
    match items.len() {
        1 => tool_icon_path(&items[0].name),
        _ => "icons/wrench.svg",
    }
}

fn collapsed_tools_label(items: &[ToolLine]) -> String {
    if let Some(current) = items.iter().rev().find(|item| item.running) {
        return tool_activity_label(&current.name);
    }
    match items.len() {
        0 => "Using tools…".into(),
        1 => tool_done_label(&items[0].name),
        n => format!("Used {n} tools"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_tools_share_a_group_until_text() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("plan");
        transcript.start_tool("get_accessibility_snapshot");
        transcript.finish_tool("get_accessibility_snapshot");
        transcript.start_tool("ui_press");
        transcript.finish_tool("ui_press");
        transcript.push_text("Done.");
        transcript.start_tool("read_file");
        assert_eq!(transcript.blocks().len(), 4);
        assert!(matches!(
            transcript.blocks()[0],
            TranscriptBlock::Thinking { .. }
        ));
        match &transcript.blocks()[1] {
            TranscriptBlock::Tools { items, expanded } => {
                assert_eq!(items.len(), 2);
                assert!(!*expanded);
                assert_eq!(items[0].name, "get_accessibility_snapshot");
                assert_eq!(items[1].name, "ui_press");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            &transcript.blocks()[2],
            TranscriptBlock::Text { text } if text == "Done."
        ));
        match &transcript.blocks()[3] {
            TranscriptBlock::Tools { items, .. } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].name, "read_file");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn collapsed_tools_show_latest_running_then_summary() {
        let mut transcript = Transcript::new();
        transcript.start_tool("get_accessibility_snapshot");
        assert_eq!(
            transcript.blocks()[0].collapsed_label(),
            "Looking at the screen…"
        );
        transcript.finish_tool("get_accessibility_snapshot");
        transcript.start_tool("ui_press");
        assert_eq!(
            transcript.blocks()[0].collapsed_label(),
            "Pressing a control…"
        );
        transcript.finish_tool("ui_press");
        assert_eq!(transcript.blocks()[0].collapsed_label(), "Used 2 tools");
    }

    #[test]
    fn thinking_stays_separate_from_tools() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("hmm");
        transcript.start_tool("read_file");
        assert_eq!(transcript.blocks().len(), 2);
        assert_eq!(transcript.blocks()[0].collapsed_label(), "Thought");
        transcript.toggle(0);
        match &transcript.blocks()[0] {
            TranscriptBlock::Thinking { expanded, text } => {
                assert!(*expanded);
                assert_eq!(text, "hmm");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn thinking_between_tools_does_not_split_the_group() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("first");
        transcript.start_tool("get_accessibility_snapshot");
        transcript.finish_tool("get_accessibility_snapshot");
        transcript.push_reasoning(" more");
        transcript.start_tool("ui_press");
        transcript.finish_tool("ui_press");
        assert_eq!(transcript.blocks().len(), 2);
        match &transcript.blocks()[0] {
            TranscriptBlock::Thinking { text, .. } => assert_eq!(text, "first more"),
            other => panic!("{other:?}"),
        }
        match &transcript.blocks()[1] {
            TranscriptBlock::Tools { items, .. } => {
                assert_eq!(items.len(), 2);
                assert!(!items[0].running);
                assert!(!items[1].running);
                assert_eq!(transcript.blocks()[1].collapsed_label(), "Used 2 tools");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn whitespace_only_text_does_not_start_a_block() {
        let mut transcript = Transcript::new();
        transcript.push_reasoning("plan");
        transcript.push_text("\n\n\n");
        transcript.start_tool("ui_press");
        assert_eq!(transcript.blocks().len(), 2);
        assert!(matches!(
            transcript.blocks()[0],
            TranscriptBlock::Thinking { .. }
        ));
        assert!(matches!(
            transcript.blocks()[1],
            TranscriptBlock::Tools { .. }
        ));
    }

    #[test]
    fn tool_icon_matches_known_tools() {
        assert_eq!(tool_icon_path("read_file"), "icons/file.svg");
        assert_eq!(
            tool_icon_path("get_accessibility_snapshot"),
            "icons/monitor.svg"
        );
        assert_eq!(tool_icon_path("ui_press"), "icons/pointer.svg");
        assert_eq!(tool_icon_path("unknown_tool"), "icons/wrench.svg");
    }

    #[test]
    fn live_activity_follows_the_agent_phase() {
        let mut transcript = Transcript::new();
        assert_eq!(transcript.live_activity(), LiveActivity::Thinking);
        assert_eq!(transcript.live_activity().label(), "Thinking");

        transcript.push_reasoning("plan");
        assert_eq!(transcript.live_activity(), LiveActivity::Thinking);

        transcript.start_tool("get_accessibility_snapshot");
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
        transcript.start_tool("get_accessibility_snapshot");
        match &transcript.blocks()[0] {
            TranscriptBlock::Tools { items, .. } => {
                assert_eq!(tools_header_icon(items), "icons/monitor.svg");
            }
            other => panic!("{other:?}"),
        }
        transcript.finish_tool("get_accessibility_snapshot");
        transcript.start_tool("ui_press");
        match &transcript.blocks()[0] {
            TranscriptBlock::Tools { items, .. } => {
                assert_eq!(tools_header_icon(items), "icons/pointer.svg");
            }
            other => panic!("{other:?}"),
        }
        transcript.finish_tool("ui_press");
        match &transcript.blocks()[0] {
            TranscriptBlock::Tools { items, .. } => {
                assert_eq!(tools_header_icon(items), "icons/wrench.svg");
            }
            other => panic!("{other:?}"),
        }
    }
}
