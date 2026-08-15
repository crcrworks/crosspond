use crosspond_core::{
    AgentEvent, ApprovalId, CommandSender, CommandWindowState, ContextCapsule, RuntimeCommand,
    StartTaskRequest, TaskId,
};
use gpui::{
    App, Context, Entity, FocusHandle, KeyBinding, Window, actions, div, prelude::*, rgb, size,
};

use crate::text_input::TextInput;
use crate::transcript::{
    Transcript, TranscriptBlock, tool_activity_label, tool_done_label, tool_icon_path,
    tools_header_icon,
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
}

struct PendingApproval {
    id: ApprovalId,
    title: String,
    description: String,
}

impl CommandWindow {
    pub fn new(commands: CommandSender, cx: &mut Context<Self>) -> Self {
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
            }
            AgentEvent::AssistantDelta { task_id, text } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.transcript.push_text(&text);
                self.state = CommandWindowState::Running;
            }
            AgentEvent::ReasoningDelta { task_id, text } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.transcript.push_reasoning(&text);
                self.state = CommandWindowState::Running;
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
                self.transcript.start_tool(&tool);
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
                self.transcript.finish_tool(&tool);
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

    fn status_line(&self) -> Option<String> {
        match self.state {
            CommandWindowState::Idle => None,
            CommandWindowState::PreparingContext => Some("Preparing…".into()),
            CommandWindowState::Running => Some("Working…".into()),
            CommandWindowState::WaitingApproval => Some("Waiting for approval…".into()),
            CommandWindowState::Completed | CommandWindowState::Failed => None,
            CommandWindowState::Cancelled => Some("Cancelled".into()),
        }
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
        let status = self.status_line();
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
                            .child(self.input.clone()),
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
                    .children(status.map(|line| {
                        div()
                            .flex_none()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(div().text_sm().text_color(muted).child(line))
                            .when(show_stop, |parent| {
                                parent.child(ui::button("stop", "Stop", dark, {
                                    let entity = cx.entity();
                                    move |event, window, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.on_stop(event, window, cx);
                                        });
                                    }
                                }))
                            })
                    }))
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
                                    .gap_1()
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

fn render_transcript_block(
    index: usize,
    block: TranscriptBlock,
    muted: gpui::Rgba,
    result_color: gpui::Rgba,
    entity: Entity<CommandWindow>,
) -> impl IntoElement {
    match block {
        TranscriptBlock::Thinking { text, expanded } => {
            let label = if text.is_empty() {
                "Thinking…".to_string()
            } else {
                "Thought".to_string()
            };
            let details = if expanded {
                vec![(None, text)]
            } else {
                Vec::new()
            };
            collapsible_block(
                ("think", index),
                if expanded { "▾" } else { "▸" },
                None,
                label,
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
            let details = if expanded {
                items
                    .iter()
                    .map(|item| {
                        let label = if item.running {
                            tool_activity_label(&item.name)
                        } else {
                            tool_done_label(&item.name)
                        };
                        (Some(tool_icon_path(&item.name)), label)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            collapsible_block(
                ("tools", index),
                if expanded { "▾" } else { "▸" },
                Some(icon),
                header,
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
            .text_color(result_color)
            .child(text)
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
    header: String,
    muted: gpui::Rgba,
    details: Vec<(Option<&'static str>, String)>,
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
                .cursor_pointer()
                .hover(|this| this.opacity(0.8))
                .on_click(move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.toggle_block(index, cx);
                    });
                })
                .child(div().flex_none().text_sm().text_color(muted).child(caret))
                .children(icon.map(|path| ui::svg_icon(path, muted)))
                .child(
                    div()
                        .flex_none()
                        .text_sm()
                        .text_color(muted)
                        .whitespace_nowrap()
                        .child(header),
                ),
        )
        .children(
            details
                .into_iter()
                .filter(|(_, body)| !body.trim().is_empty())
                .map(|(row_icon, body)| {
                    div()
                        .flex()
                        .flex_none()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .pl_4()
                        .children(row_icon.map(|path| ui::svg_icon(path, muted)))
                        .child(
                            div()
                                .flex_none()
                                .whitespace_normal()
                                .text_sm()
                                .text_color(muted)
                                .child(body),
                        )
                }),
        )
}
