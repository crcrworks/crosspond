use std::sync::Arc;
use std::time::{Duration, Instant};

use crosspond_core::{
    AgentEvent, ApprovalId, CommandSender, CommandWindowState, ComputerApprovalMode, ConfigStore,
    ContextCapsule, RuntimeCommand, StartTaskRequest, TaskId,
};
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, KeyBinding, SharedString, Timer, Window,
    actions, div, prelude::*, rems, rgb, size,
};

use crate::activity_label::activity_label;
use crate::text_input::TextInput;
use crate::transcript::{
    LiveActivity, Transcript, TranscriptBlock, tool_activity_label, tool_done_label,
    tool_icon_path, tools_header_icon,
};
use crate::ui;

actions!(command_window, [Submit, HideLauncher]);

const ASK_PLACEHOLDER: &str = "Ask or do anything...";
const FOLLOW_UP_PLACEHOLDER: &str = "Ask a follow-up...";

pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", Submit, Some("CommandWindow")),
        KeyBinding::new("escape", HideLauncher, Some("CommandWindow")),
    ]
}

pub struct CommandWindow {
    input: Entity<TextInput>,
    state: CommandWindowState,
    prompt: String,
    transcript: Transcript,
    artifacts: Vec<String>,
    ambient: ContextCapsule,
    current_task: Option<TaskId>,
    pending_approval: Option<PendingApproval>,
    commands: CommandSender,
    config: Arc<dyn ConfigStore>,
    computer_approval: ComputerApprovalMode,
    tool_starts: Vec<(String, Instant)>,
    activity: LiveActivity,
}

struct PendingApproval {
    id: ApprovalId,
    title: String,
    description: String,
}

impl CommandWindow {
    pub fn new(
        commands: CommandSender,
        config: Arc<dyn ConfigStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let computer_approval = config
            .load()
            .map(|loaded| loaded.computer_approval)
            .unwrap_or_default();
        let input = cx.new(|cx| TextInput::new(ASK_PLACEHOLDER, cx));
        Self {
            input,
            state: CommandWindowState::Idle,
            prompt: String::new(),
            transcript: Transcript::new(),
            artifacts: Vec::new(),
            ambient: ContextCapsule::default(),
            current_task: None,
            pending_approval: None,
            commands,
            config,
            computer_approval,
            tool_starts: Vec::new(),
            activity: LiveActivity::Thinking,
        }
    }

    pub fn input_focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle_clone()
    }

    pub fn apply_event(&mut self, event: AgentEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            AgentEvent::TaskStarted { task_id, prompt } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.prompt = prompt;
                self.state = CommandWindowState::PreparingContext;
                self.activity = LiveActivity::Thinking;
            }
            AgentEvent::AssistantDelta { task_id, text } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.transcript.push_text(&text);
                self.state = CommandWindowState::Running;
                self.activity = LiveActivity::Writing;
            }
            AgentEvent::ReasoningDelta { task_id, text } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.transcript.push_reasoning(&text);
                self.state = CommandWindowState::Running;
                self.activity = LiveActivity::Thinking;
            }
            AgentEvent::TaskCompleted { task_id, summary } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                if !self.transcript.has_assistant_text() && !summary.trim().is_empty() {
                    self.transcript.push_text(&summary);
                }
                self.state = CommandWindowState::Completed;
                self.pending_approval = None;
                self.input.update(cx, |input, cx| {
                    input.reset();
                    input.set_placeholder(FOLLOW_UP_PLACEHOLDER);
                    cx.notify();
                });
            }
            AgentEvent::TaskFailed { task_id, message } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.transcript.push_notice(&message);
                self.state = CommandWindowState::Failed;
                self.pending_approval = None;
            }
            AgentEvent::TaskCancelled { task_id } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.state = CommandWindowState::Cancelled;
                self.pending_approval = None;
            }
            AgentEvent::ToolStarted { task_id, tool } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.tool_starts.push((tool.clone(), Instant::now()));
                self.transcript.start_tool(&tool);
                self.activity = LiveActivity::Tool(tool);
            }
            AgentEvent::ArtifactCreated {
                task_id,
                display_name,
            } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.artifacts.push(display_name);
            }
            AgentEvent::ToolFinished { task_id, tool } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.finish_tool_after_minimum_display(tool, cx);
            }
            AgentEvent::ConnectionTested { .. } => return,
            AgentEvent::ApprovalRequired {
                task_id,
                approval_id,
                title,
                description,
            } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.pending_approval = Some(PendingApproval {
                    id: approval_id,
                    title,
                    description,
                });
                self.state = CommandWindowState::WaitingApproval;
            }
            AgentEvent::ContextCollected { task_id } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.state = CommandWindowState::Running;
                self.activity = LiveActivity::Thinking;
            }
        }
        self.sync_window_size(window);
        cx.notify();
    }

    pub fn reset_session(&mut self, cx: &mut Context<Self>) {
        self.cancel_if_running();
        self.commands.send(RuntimeCommand::ResetSession);
        self.state = CommandWindowState::Idle;
        self.prompt.clear();
        self.transcript.clear();
        self.artifacts.clear();
        self.ambient = ContextCapsule::default();
        self.current_task = None;
        self.pending_approval = None;
        self.tool_starts.clear();
        self.activity = LiveActivity::Thinking;
        self.input.update(cx, |input, cx| {
            input.reset();
            input.set_placeholder(ASK_PLACEHOLDER);
            cx.notify();
        });
        cx.notify();
    }

    pub fn set_ambient_context(
        &mut self,
        ambient: ContextCapsule,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ambient = ambient;
        self.sync_window_size(window);
        cx.notify();
    }

    fn is_busy(&self) -> bool {
        matches!(
            self.state,
            CommandWindowState::Running
                | CommandWindowState::PreparingContext
                | CommandWindowState::WaitingApproval
        )
    }

    fn cancel_if_running(&mut self) {
        if self.is_busy()
            && let Some(task_id) = self.current_task
        {
            self.commands.send(RuntimeCommand::Cancel(task_id));
        }
    }

    fn on_submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(
            self.state,
            CommandWindowState::Idle
                | CommandWindowState::Completed
                | CommandWindowState::Failed
                | CommandWindowState::Cancelled
        ) {
            return;
        }

        let prompt = self.input.read(cx).text().trim().to_string();
        if prompt.is_empty() {
            return;
        }

        let task_id = TaskId::new();
        self.current_task = Some(task_id);
        self.prompt = prompt.clone();
        self.transcript.clear();
        self.artifacts.clear();
        self.pending_approval = None;
        self.activity = LiveActivity::Thinking;
        self.state = CommandWindowState::PreparingContext;
        self.commands
            .send(RuntimeCommand::StartTask(StartTaskRequest {
                task_id,
                prompt,
                context: self.ambient.clone(),
            }));
        self.input.update(cx, |input, cx| {
            input.reset();
            cx.notify();
        });
        self.sync_window_size(window);
        cx.notify();
    }

    fn on_escape(&mut self, _: &HideLauncher, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            self.cancel_if_running();
            return;
        }
        self.reset_session(cx);
        self.sync_window_size(window);
        crate::launcher::mark_hidden(cx);
        cx.hide();
    }

    fn on_stop(&mut self, _: &gpui::ClickEvent, _: &mut Window, _: &mut Context<Self>) {
        self.cancel_if_running();
    }

    fn on_cycle_computer_approval(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.computer_approval = self.computer_approval.cycle();
        if let Ok(mut loaded) = self.config.load() {
            loaded.computer_approval = self.computer_approval;
            let _ = self.config.save(&loaded);
        }
        cx.notify();
    }

    fn on_allow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pending) = self.pending_approval.take() {
            self.commands.send(RuntimeCommand::Approve(pending.id));
            self.state = CommandWindowState::Running;
            self.sync_window_size(window);
            cx.notify();
        }
    }

    fn on_reject_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pending) = self.pending_approval.take() {
            self.commands.send(RuntimeCommand::Reject(pending.id));
            self.state = CommandWindowState::Running;
            self.sync_window_size(window);
            cx.notify();
        }
    }

    fn sync_window_size(&self, window: &mut Window) {
        let current = window.viewport_size();
        let min_width = crate::launcher::WINDOW_WIDTH;
        let min_height = match self.state {
            CommandWindowState::Idle => {
                crate::launcher::idle_height(self.ambient.badge_lines().len())
            }
            _ => crate::launcher::RESULT_HEIGHT,
        };
        if self.state == CommandWindowState::Idle {
            window.resize(size(min_width, min_height));
            return;
        }
        if current.width < min_width || current.height < min_height {
            window.resize(size(
                current.width.max(min_width),
                current.height.max(min_height),
            ));
        }
    }

    fn take_tool_start(&mut self, name: &str) -> Option<Instant> {
        if let Some(index) = self.tool_starts.iter().rposition(|(tool, _)| tool == name) {
            Some(self.tool_starts.remove(index).1)
        } else {
            self.tool_starts.pop().map(|(_, started)| started)
        }
    }

    fn finish_tool_after_minimum_display(&mut self, tool: String, cx: &mut Context<Self>) {
        let elapsed = self
            .take_tool_start(&tool)
            .map(|started| started.elapsed())
            .unwrap_or(Duration::MAX);
        const MIN_RUNNING: Duration = Duration::from_millis(800);
        if elapsed >= MIN_RUNNING {
            self.transcript.finish_tool(&tool);
            self.activity = LiveActivity::PreparingNextMoves;
            return;
        }
        let wait = MIN_RUNNING - elapsed;
        let task_id = self.current_task;
        cx.spawn(async move |this, cx| {
            Timer::after(wait).await;
            this.update(cx, |this, cx| {
                if this.current_task != task_id {
                    return;
                }
                this.transcript.finish_tool(&tool);
                this.activity = LiveActivity::PreparingNextMoves;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_block(&mut self, index: usize, cx: &mut Context<Self>) {
        self.transcript.toggle(index);
        cx.notify();
    }
}

impl gpui::Render for CommandWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = ui::is_dark(window);
        let bg = if dark { rgb(0x1c1c1e) } else { rgb(0xffffff) };
        let border = if dark { rgb(0x3a3a3c) } else { rgb(0xd2d2d7) };
        let text = if dark { rgb(0xf5f5f7) } else { rgb(0x1d1d1f) };
        let muted = if dark { rgb(0x8e8e93) } else { rgb(0x6e6e73) };
        let live_thinking = (self.activity == LiveActivity::Thinking)
            .then(|| self.transcript.live_thinking_index())
            .flatten();
        let status = heartbeat_status(self.state, &self.transcript, &self.activity);
        let prompt_label = (!self.prompt.is_empty() && self.state != CommandWindowState::Idle)
            .then(|| self.prompt.clone());
        let artifacts = self.artifacts.clone();
        let blocks: Vec<(usize, TranscriptBlock)> = self
            .transcript
            .blocks()
            .iter()
            .cloned()
            .enumerate()
            .collect();
        let result_color = if self.state == CommandWindowState::Failed {
            rgb(0xff453a)
        } else {
            text
        };
        let show_stop = matches!(
            self.state,
            CommandWindowState::Running | CommandWindowState::PreparingContext
        );
        let mode_label = self.computer_approval.button_label();
        let badges = self.ambient.badge_lines();
        let approval = self
            .pending_approval
            .as_ref()
            .map(|pending| (pending.title.clone(), pending.description.clone()));
        let entity = cx.entity();

        div()
            .key_context("CommandWindow")
            .on_action(cx.listener(Self::on_submit))
            .on_action(cx.listener(Self::on_escape))
            .flex()
            .flex_col()
            .size_full()
            .p_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .rounded_xl()
                    .border_1()
                    .border_color(border)
                    .bg(bg)
                    .shadow_lg()
                    .text_color(text)
                    .px_4()
                    .py_3()
                    .gap_2()
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(div().size_2().rounded_full().bg(rgb(0x30d158)))
                            .child(div().flex_1().min_w_0().child(self.input.clone()))
                            .child(ui::button("ui-mode", mode_label, dark, {
                                let entity = entity.clone();
                                move |event, window, cx| {
                                    entity.update(cx, |this, cx| {
                                        this.on_cycle_computer_approval(event, window, cx);
                                    });
                                }
                            }))
                            .when(show_stop, |parent| {
                                parent.child(ui::button("stop", "Stop", dark, {
                                    let entity = entity.clone();
                                    move |event, window, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.on_stop(event, window, cx);
                                        });
                                    }
                                }))
                            }),
                    )
                    .children(
                        badges
                            .into_iter()
                            .map(|line| div().flex_none().text_xs().text_color(muted).child(line)),
                    )
                    .children(
                        prompt_label.map(|prompt| {
                            div().flex_none().text_sm().text_color(muted).child(prompt)
                        }),
                    )
                    .child(
                        div()
                            .id("transcript")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_none()
                                    .w_full()
                                    .justify_start()
                                    .gap_0()
                                    .children(blocks.into_iter().filter_map(|(index, block)| {
                                        if matches!(
                                            &block,
                                            TranscriptBlock::Text { text }
                                                if text.trim().is_empty()
                                        ) {
                                            return None;
                                        }
                                        Some(render_transcript_block(
                                            index,
                                            block,
                                            live_thinking == Some(index),
                                            muted,
                                            result_color,
                                            entity.clone(),
                                        ))
                                    }))
                                    .children(artifacts.into_iter().map(|name| {
                                        div()
                                            .flex_none()
                                            .text_sm()
                                            .text_color(muted)
                                            .child(format!("Created {name}"))
                                    }))
                                    .children(status.map(|line| activity_heartbeat(line, muted)))
                                    .children(approval.map(|(title, description)| {
                                        render_approval_card(
                                            title,
                                            description,
                                            muted,
                                            dark,
                                            entity.clone(),
                                        )
                                    })),
                            ),
                    ),
            )
    }
}

fn heartbeat_status(
    state: CommandWindowState,
    transcript: &Transcript,
    activity: &LiveActivity,
) -> Option<String> {
    match state {
        CommandWindowState::Idle
        | CommandWindowState::Completed
        | CommandWindowState::Failed
        | CommandWindowState::Cancelled
        | CommandWindowState::WaitingApproval => None,
        CommandWindowState::PreparingContext => Some("Gathering context".into()),
        CommandWindowState::Running => {
            if transcript.running_tool().is_some() {
                return None;
            }
            match activity {
                LiveActivity::Writing | LiveActivity::Tool(_) => None,
                LiveActivity::Thinking if transcript.live_thinking_index().is_some() => None,
                LiveActivity::Thinking | LiveActivity::PreparingNextMoves => Some(activity.label()),
            }
        }
    }
}

fn activity_heartbeat(line: String, muted: gpui::Rgba) -> impl IntoElement {
    div()
        .flex_none()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .text_sm()
        .text_color(muted)
        .child(activity_label("status-line", line, true, muted))
}

fn render_transcript_block(
    index: usize,
    block: TranscriptBlock,
    thinking_live: bool,
    muted: gpui::Rgba,
    result_color: gpui::Rgba,
    entity: Entity<CommandWindow>,
) -> impl IntoElement {
    match block {
        TranscriptBlock::Thinking { text, expanded } => {
            let label = if thinking_live {
                "Thinking".to_string()
            } else {
                "Thought".to_string()
            };
            let body = text.trim().to_string();
            let details = if expanded && !body.is_empty() {
                vec![
                    div()
                        .pl_4()
                        .whitespace_normal()
                        .text_sm()
                        .line_height(rems(1.35))
                        .text_color(muted)
                        .child(body)
                        .into_any_element(),
                ]
            } else {
                Vec::new()
            };
            collapsible_block(
                ("think", index),
                if expanded { "▾" } else { "▸" },
                None,
                activity_label(("think-header", index), label, thinking_live, muted),
                muted,
                details,
                entity,
            )
            .into_any_element()
        }
        TranscriptBlock::Tools { items, expanded } => {
            let header = TranscriptBlock::Tools {
                items: items.clone(),
                expanded,
            }
            .collapsed_label();
            let icon = tools_header_icon(&items);
            let running = items.iter().any(|item| item.running);
            let details = if expanded {
                items
                    .iter()
                    .enumerate()
                    .map(|(row, item)| {
                        let label = if item.running {
                            tool_activity_label(&item.name)
                        } else {
                            tool_done_label(&item.name)
                        };
                        tool_detail_row(
                            index,
                            row,
                            tool_icon_path(&item.name),
                            label,
                            item.running,
                            muted,
                        )
                    })
                    .collect()
            } else {
                Vec::new()
            };
            collapsible_block(
                ("tools", index),
                if expanded { "▾" } else { "▸" },
                Some(icon),
                activity_label(("tool-header", index), header, running, muted),
                muted,
                details,
                entity,
            )
            .into_any_element()
        }
        TranscriptBlock::Text { text } => div()
            .flex_none()
            .whitespace_normal()
            .text_sm()
            .line_height(rems(1.35))
            .text_color(result_color)
            .child(text.trim_end().to_string())
            .into_any_element(),
    }
}

fn render_approval_card(
    title: String,
    description: String,
    muted: gpui::Rgba,
    dark: bool,
    entity: Entity<CommandWindow>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap_2()
        .pt_2()
        .child(div().text_sm().child("Crosspond wants to:"))
        .child(div().text_sm().child(title))
        .children(
            (!description.is_empty()).then(|| div().text_sm().text_color(muted).child(description)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(ui::button("approval-allow", "Allow", dark, {
                    let entity = entity.clone();
                    move |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            this.on_allow(window, cx);
                        });
                    }
                }))
                .child(ui::button("approval-cancel", "Cancel", dark, {
                    move |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            this.on_reject_action(window, cx);
                        });
                    }
                })),
        )
}

fn collapsible_block(
    id: (&'static str, usize),
    caret: &'static str,
    icon: Option<&'static str>,
    header: impl IntoElement,
    muted: gpui::Rgba,
    details: Vec<AnyElement>,
    entity: Entity<CommandWindow>,
) -> impl IntoElement {
    let index = id.1;
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap_1()
        .child(
            div()
                .id(id)
                .flex()
                .flex_none()
                .flex_row()
                .items_center()
                .gap_1()
                .w_full()
                .cursor_pointer()
                .hover(|this| this.opacity(0.8))
                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.toggle_block(index, cx);
                    });
                })
                .child(div().flex_none().text_sm().text_color(muted).child(caret))
                .children(icon.map(|path| ui::svg_icon(path, muted)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_sm()
                        .text_color(muted)
                        .whitespace_nowrap()
                        .child(header),
                ),
        )
        .children(details)
}

fn tool_detail_row(
    block: usize,
    row: usize,
    icon: &'static str,
    label: String,
    running: bool,
    muted: gpui::Rgba,
) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .flex_row()
        .items_center()
        .gap_1()
        .pl_4()
        .w_full()
        .child(ui::svg_icon(icon, muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_sm()
                .text_color(muted)
                .child(activity_label(
                    (SharedString::from(format!("tool-row-{block}")), row),
                    label,
                    running,
                    muted,
                )),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(
        state: CommandWindowState,
        transcript: &Transcript,
        activity: LiveActivity,
    ) -> Option<String> {
        heartbeat_status(state, transcript, &activity)
    }

    #[test]
    fn heartbeat_hides_when_idle_done_or_writing() {
        let mut transcript = Transcript::new();
        let thinking = LiveActivity::Thinking;
        assert_eq!(
            status(CommandWindowState::Idle, &transcript, thinking.clone()),
            None
        );
        assert_eq!(
            status(CommandWindowState::Completed, &transcript, thinking.clone()),
            None
        );
        assert_eq!(
            status(CommandWindowState::Failed, &transcript, thinking.clone()),
            None
        );
        assert_eq!(
            status(CommandWindowState::Cancelled, &transcript, thinking.clone()),
            None
        );
        assert_eq!(
            status(
                CommandWindowState::WaitingApproval,
                &transcript,
                thinking.clone()
            ),
            None
        );
        transcript.push_text("Done.");
        assert_eq!(
            status(
                CommandWindowState::Running,
                &transcript,
                LiveActivity::Writing
            ),
            None
        );
    }

    #[test]
    fn heartbeat_shows_when_the_screen_would_otherwise_sit_still() {
        let mut transcript = Transcript::new();
        assert_eq!(
            status(
                CommandWindowState::PreparingContext,
                &transcript,
                LiveActivity::Thinking
            ),
            Some("Gathering context".into())
        );
        assert_eq!(
            status(
                CommandWindowState::Running,
                &transcript,
                LiveActivity::Thinking
            ),
            Some("Thinking".into())
        );
        transcript.start_tool("read_file");
        assert_eq!(
            status(
                CommandWindowState::Running,
                &transcript,
                LiveActivity::Tool("read_file".into())
            ),
            None
        );
        transcript.finish_tool("read_file");
        assert_eq!(
            status(
                CommandWindowState::Running,
                &transcript,
                LiveActivity::PreparingNextMoves
            ),
            Some("Preparing next moves".into())
        );
        assert_eq!(
            status(
                CommandWindowState::Running,
                &transcript,
                LiveActivity::Thinking
            ),
            Some("Thinking".into())
        );
        transcript.push_reasoning("plan");
        assert_eq!(
            status(
                CommandWindowState::Running,
                &transcript,
                LiveActivity::Thinking
            ),
            None
        );
    }
}
