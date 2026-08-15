use crosspond_core::{
    AgentEvent, ApprovalId, CommandSender, CommandWindowState, ContextCapsule, RuntimeCommand,
    StartTaskRequest, TaskId,
};
use gpui::{
    App, Context, Entity, FocusHandle, KeyBinding, Window, actions, div, prelude::*, rgb, size,
};

use crate::text_input::TextInput;
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
    result: Option<String>,
    activity: Vec<String>,
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
            result: None,
            activity: Vec::new(),
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
                self.result.get_or_insert_with(String::new).push_str(&text);
                self.state = CommandWindowState::Running;
            }
            AgentEvent::TaskCompleted { task_id, summary } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                if self.result.as_deref().unwrap_or("").is_empty() {
                    self.result = Some(summary);
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
                self.result = Some(message);
                self.state = CommandWindowState::Failed;
                self.pending_approval = None;
            }
            AgentEvent::TaskCancelled { task_id } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.state = CommandWindowState::Cancelled;
                self.result = Some("Cancelled".into());
                self.pending_approval = None;
            }
            AgentEvent::ToolStarted { task_id, tool } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.activity.push(tool_activity_label(&tool));
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
            AgentEvent::ToolFinished { task_id, .. } => {
                if self.current_task != Some(task_id) {
                    return;
                }
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
        self.result = None;
        self.activity.clear();
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
        self.result = None;
        self.activity.clear();
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
        let height = match self.state {
            CommandWindowState::Idle => {
                crate::launcher::idle_height(self.ambient.badge_lines().len())
            }
            _ => crate::launcher::RESULT_HEIGHT,
        };
        window.resize(size(crate::launcher::WINDOW_WIDTH, height));
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

    fn result_text(&self) -> Option<&str> {
        self.result.as_deref().filter(|text| !text.is_empty())
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
        let result = self.result_text().map(str::to_string);
        let activity = self.activity.clone();
        let artifacts = self.artifacts.clone();
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
                            .map(|line| div().text_xs().text_color(muted).child(line)),
                    )
                    .children(
                        prompt_label.map(|prompt| div().text_sm().text_color(muted).child(prompt)),
                    )
                    .children(status.map(|line| {
                        div()
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
                    .children(approval.map(|(title, description)| {
                        let entity = cx.entity();
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().child("Crosspond wants to:"))
                            .child(div().text_sm().child(title))
                            .children(
                                (!description.is_empty())
                                    .then(|| div().text_sm().text_color(muted).child(description)),
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
                    }))
                    .children(
                        activity
                            .into_iter()
                            .map(|line| div().text_sm().text_color(muted).child(line)),
                    )
                    .children(artifacts.into_iter().map(|name| {
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("Created {name}"))
                    }))
                    .children(result.map(|line| {
                        div()
                            .id("result")
                            .flex_1()
                            .overflow_y_scroll()
                            .whitespace_normal()
                            .text_sm()
                            .text_color(result_color)
                            .child(line)
                    })),
            )
    }
}

fn tool_activity_label(name: &str) -> String {
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
