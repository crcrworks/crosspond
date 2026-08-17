use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crosspond_core::{
    AgentEvent, ApprovalId, CommandSender, CommandWindowState, ComputerApprovalMode, ConfigStore,
    ContextCapsule, Receipt, RuntimeCommand, StartTaskRequest, TaskHistoryEntry, TaskId,
    default_tasks_root, history_group_label, list_recent_tasks,
};
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, KeyBinding, SharedString, Timer, Window,
    actions, div, prelude::*, rems, rgb, size,
};

use crate::activity_label::activity_label;
use crate::markdown::{self, MarkdownPalette};
use crate::text_input::TextInput;
use crate::transcript::{
    LiveActivity, Transcript, TranscriptBlock, tool_activity_label, tool_done_label,
    tool_icon_path, tools_header_icon,
};
use crate::ui;

actions!(command_window, [Submit, HideLauncher, OpenHistory]);

const ASK_PLACEHOLDER: &str = "Ask or do anything...";
const FOLLOW_UP_PLACEHOLDER: &str = "Ask a follow-up...";

pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", Submit, Some("CommandWindow")),
        KeyBinding::new("escape", HideLauncher, Some("CommandWindow")),
        KeyBinding::new("up", OpenHistory, Some("CommandWindow")),
    ]
}

enum Overlay {
    None,
    Onboarding {
        ready: bool,
        hint: Option<String>,
    },
    History {
        entries: Vec<TaskHistoryEntry>,
        selected: Option<usize>,
    },
}

#[derive(Clone)]
struct ArtifactItem {
    name: String,
    path: PathBuf,
}

pub struct CommandWindow {
    input: Entity<TextInput>,
    state: CommandWindowState,
    prompt: String,
    transcript: Transcript,
    artifacts: Vec<ArtifactItem>,
    receipt: Option<Receipt>,
    overlay: Overlay,
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
            receipt: None,
            overlay: Overlay::None,
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
            AgentEvent::TaskCompleted {
                task_id,
                summary,
                receipt,
            } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                if !self.transcript.has_assistant_text() && !summary.trim().is_empty() {
                    self.transcript.push_text(&summary);
                }
                self.receipt = Some(receipt);
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
                path,
            } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.artifacts.push(ArtifactItem {
                    name: display_name,
                    path,
                });
            }
            AgentEvent::ToolFinished { task_id, tool } => {
                if self.current_task != Some(task_id) {
                    return;
                }
                self.finish_tool_after_minimum_display(tool, cx);
            }
            AgentEvent::ConnectionTested { ok, .. } => {
                if let Overlay::Onboarding { ready, hint } = &mut self.overlay {
                    if ok {
                        *ready = true;
                        *hint = None;
                    }
                    cx.notify();
                }
                return;
            }
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
        self.receipt = None;
        self.overlay = Overlay::None;
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

    pub fn enter_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let ready = crate::settings::has_provider_key(cx);
        self.overlay = Overlay::Onboarding { ready, hint: None };
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
        if matches!(self.overlay, Overlay::Onboarding { .. }) {
            return;
        }
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
        self.receipt = None;
        self.overlay = Overlay::None;
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
        if let Overlay::History {
            selected: Some(_), ..
        } = &self.overlay
        {
            if let Overlay::History { selected, .. } = &mut self.overlay {
                *selected = None;
            }
            self.sync_window_size(window);
            cx.notify();
            return;
        }
        if matches!(self.overlay, Overlay::History { .. }) {
            self.overlay = Overlay::None;
            self.sync_window_size(window);
            cx.notify();
            return;
        }
        self.reset_session(cx);
        self.sync_window_size(window);
        crate::launcher::mark_hidden(cx);
        cx.hide();
    }

    fn on_open_history(&mut self, _: &OpenHistory, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || matches!(self.overlay, Overlay::Onboarding { .. }) {
            return;
        }
        if !self.input.read(cx).text().is_empty()
            && matches!(self.overlay, Overlay::None)
            && self.state == CommandWindowState::Idle
        {
            return;
        }
        self.show_history(window, cx);
    }

    fn show_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entries = list_recent_tasks(&default_tasks_root(), 50);
        self.overlay = Overlay::History {
            entries,
            selected: None,
        };
        self.sync_window_size(window);
        cx.notify();
    }

    fn select_history(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Overlay::History {
            entries, selected, ..
        } = &mut self.overlay
            && index < entries.len()
        {
            *selected = Some(index);
            cx.notify();
        }
    }

    fn close_history_selection(&mut self, cx: &mut Context<Self>) {
        if let Overlay::History { selected, .. } = &mut self.overlay {
            *selected = None;
            cx.notify();
        }
    }

    fn on_history_button(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy() || matches!(self.overlay, Overlay::Onboarding { .. }) {
            return;
        }
        if matches!(self.overlay, Overlay::History { .. }) {
            self.overlay = Overlay::None;
            self.sync_window_size(window);
            cx.notify();
            return;
        }
        self.show_history(window, cx);
    }

    fn continue_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::settings::has_provider_key(cx) {
            self.overlay = Overlay::Onboarding {
                ready: true,
                hint: None,
            };
        } else {
            if let Overlay::Onboarding { hint, .. } = &mut self.overlay {
                *hint = Some("Add an API key in Settings first.".into());
            }
            crate::settings::open(cx);
        }
        self.sync_window_size(window);
        cx.notify();
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
        let min_height =
            if self.state == CommandWindowState::Idle && matches!(self.overlay, Overlay::None) {
                crate::launcher::idle_height(self.ambient.badge_lines().len())
            } else {
                crate::launcher::RESULT_HEIGHT
            };
        if self.state == CommandWindowState::Idle && matches!(self.overlay, Overlay::None) {
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
        let prompt_label = (!self.prompt.is_empty()
            && self.state != CommandWindowState::Idle
            && matches!(self.overlay, Overlay::None))
        .then(|| self.prompt.clone());
        let artifacts = self.artifacts.clone();
        let receipt = self.receipt.clone();
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
        let onboarding = matches!(self.overlay, Overlay::Onboarding { .. });
        let show_history = !self.is_busy() && !onboarding;
        let failed_settings = self.state == CommandWindowState::Failed
            && matches!(self.overlay, Overlay::None)
            && self
                .transcript
                .blocks()
                .iter()
                .any(|block| matches!(block, TranscriptBlock::Text { text } if failed_offers_settings(text)));
        let mode_label = self.computer_approval.button_label();
        let badges = if onboarding {
            Vec::new()
        } else {
            self.ambient.badge_lines()
        };
        let approval = self
            .pending_approval
            .as_ref()
            .map(|pending| (pending.title.clone(), pending.description.clone()));
        let entity = cx.entity();
        let body = match &self.overlay {
            Overlay::Onboarding { ready, hint } => {
                render_onboarding(*ready, hint.clone(), muted, dark, entity.clone())
            }
            Overlay::History { entries, selected } => {
                render_history(entries, *selected, muted, dark, entity.clone())
            }
            Overlay::None => render_task_body(
                blocks,
                live_thinking,
                artifacts,
                receipt,
                status,
                approval,
                failed_settings,
                muted,
                result_color,
                dark,
                entity.clone(),
            ),
        };

        div()
            .key_context("CommandWindow")
            .on_action(cx.listener(Self::on_submit))
            .on_action(cx.listener(Self::on_escape))
            .on_action(cx.listener(Self::on_open_history))
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
                            .when(onboarding, |row| {
                                row.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_sm()
                                        .child("Welcome to Crosspond"),
                                )
                            })
                            .when(!onboarding, |row| {
                                row.child(div().flex_1().min_w_0().child(self.input.clone()))
                                    .child(ui::button("ui-mode", mode_label, dark, {
                                        let entity = entity.clone();
                                        move |event, window, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.on_cycle_computer_approval(event, window, cx);
                                            });
                                        }
                                    }))
                            })
                            .when(show_history, |row| {
                                row.child(ui::button("history", "History", dark, {
                                    let entity = entity.clone();
                                    move |event, window, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.on_history_button(event, window, cx);
                                        });
                                    }
                                }))
                            })
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
                            .child(body),
                    ),
            )
    }
}

fn failed_offers_settings(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("settings")
        || lower.contains("api key")
        || lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("provider")
}

#[allow(clippy::too_many_arguments)]
fn render_task_body(
    blocks: Vec<(usize, TranscriptBlock)>,
    live_thinking: Option<usize>,
    artifacts: Vec<ArtifactItem>,
    receipt: Option<Receipt>,
    status: Option<String>,
    approval: Option<(String, String)>,
    failed_settings: bool,
    muted: gpui::Rgba,
    result_color: gpui::Rgba,
    dark: bool,
    entity: Entity<CommandWindow>,
) -> AnyElement {
    let show_live_artifacts = receipt.is_none();
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
                TranscriptBlock::Text { text } if text.trim().is_empty()
            ) {
                return None;
            }
            Some(render_transcript_block(
                index,
                block,
                live_thinking == Some(index),
                muted,
                result_color,
                dark,
                entity.clone(),
            ))
        }))
        .children(show_live_artifacts.then(|| {
            div()
                .flex()
                .flex_col()
                .flex_none()
                .children(artifacts.iter().map(|item| {
                    div()
                        .flex_none()
                        .text_sm()
                        .text_color(muted)
                        .child(format!("Created {}", item.name))
                }))
        }))
        .children(receipt.map(|receipt| {
            let pairs: Vec<(String, Option<PathBuf>)> = receipt
                .artifacts
                .iter()
                .map(|name| {
                    let path = artifacts
                        .iter()
                        .find(|item| item.name == *name)
                        .map(|item| item.path.clone());
                    (name.clone(), path)
                })
                .collect();
            render_receipt(receipt.actions, pairs, muted, dark)
        }))
        .children(status.map(|line| activity_heartbeat(line, muted)))
        .children(failed_settings.then(|| {
            div()
                .pt_2()
                .child(ui::button("open-settings", "Open Settings", dark, {
                    move |_, _, cx| {
                        crate::settings::open(cx);
                    }
                }))
        }))
        .children(approval.map(|(title, description)| {
            render_approval_card(title, description, muted, dark, entity.clone())
        }))
        .into_any_element()
}

fn render_receipt(
    actions: Vec<String>,
    artifacts: Vec<(String, Option<PathBuf>)>,
    muted: gpui::Rgba,
    dark: bool,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap_1()
        .pt_2()
        .child(div().text_sm().child("✓ Done"))
        .children((!actions.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_1()
                .pt_1()
                .child(div().text_sm().text_color(muted).child("Changed"))
                .children(actions.into_iter().map(|line| {
                    div()
                        .flex_none()
                        .text_sm()
                        .text_color(muted)
                        .child(format!("• {line}"))
                }))
        }))
        .children((!artifacts.is_empty()).then(move || {
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_1()
                .pt_1()
                .child(div().text_sm().text_color(muted).child("Artifacts"))
                .children(
                    artifacts
                        .into_iter()
                        .enumerate()
                        .map(|(index, (name, path))| {
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(div().flex_1().min_w_0().text_sm().child(name))
                                .children(path.map(|path| {
                                    ui::button(("show-finder", index), "Show in Finder", dark, {
                                        move |_, _, cx| {
                                            cx.reveal_path(&path);
                                        }
                                    })
                                }))
                        }),
                )
        }))
        .into_any_element()
}

fn render_onboarding(
    ready: bool,
    hint: Option<String>,
    muted: gpui::Rgba,
    dark: bool,
    entity: Entity<CommandWindow>,
) -> AnyElement {
    if ready {
        return div()
            .flex()
            .flex_col()
            .flex_none()
            .gap_3()
            .pt_2()
            .child(div().text_sm().child("Crosspond is ready."))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .whitespace_normal()
                    .child("Press Option + Space anywhere."),
            )
            .child(ui::button("onboarding-done", "Done", dark, {
                move |_, _, cx| {
                    crate::launcher::hide(cx);
                }
            }))
            .into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap_3()
        .pt_2()
        .child(div().text_sm().child("Bring your own AI."))
        .child(
            div()
                .text_sm()
                .text_color(muted)
                .whitespace_normal()
                .child(
                    "Set a provider, model, and API key in Settings. Accessibility is not required for chat.",
                ),
        )
        .children(hint.map(|line| {
            div()
                .text_sm()
                .text_color(rgb(0xff453a))
                .whitespace_normal()
                .child(line)
        }))
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(ui::button("onboarding-settings", "Open Settings", dark, {
                    move |_, _, cx| {
                        crate::settings::open(cx);
                    }
                }))
                .child(ui::button("onboarding-continue", "Continue", dark, {
                    let entity = entity.clone();
                    move |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            this.continue_onboarding(window, cx);
                        });
                    }
                })),
        )
        .into_any_element()
}

fn render_history(
    entries: &[TaskHistoryEntry],
    selected: Option<usize>,
    muted: gpui::Rgba,
    dark: bool,
    entity: Entity<CommandWindow>,
) -> AnyElement {
    if let Some(index) = selected
        && let Some(entry) = entries.get(index)
    {
        return render_history_detail(entry, muted, dark, entity);
    }
    if entries.is_empty() {
        return div()
            .flex_none()
            .pt_2()
            .text_sm()
            .text_color(muted)
            .child("No recent tasks")
            .into_any_element();
    }
    let now = SystemTime::now();
    let mut last_group: Option<&'static str> = None;
    let mut rows: Vec<AnyElement> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let group = history_group_label(entry.modified, now);
        if last_group != Some(group) {
            rows.push(
                div()
                    .flex_none()
                    .pt_2()
                    .text_xs()
                    .text_color(muted)
                    .child(group)
                    .into_any_element(),
            );
            last_group = Some(group);
        }
        let title = format!("{} {}", entry.status_mark(), entry.title());
        let entity = entity.clone();
        rows.push(
            div()
                .id(("history-item", index))
                .flex_none()
                .py_1()
                .cursor_pointer()
                .hover(|this| this.opacity(0.8))
                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.select_history(index, cx);
                    });
                })
                .child(div().text_sm().child(title))
                .into_any_element(),
        );
    }
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w_full()
        .gap_0()
        .children(rows)
        .into_any_element()
}

fn render_history_detail(
    entry: &TaskHistoryEntry,
    muted: gpui::Rgba,
    dark: bool,
    entity: Entity<CommandWindow>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w_full()
        .gap_2()
        .pt_1()
        .child(ui::button("history-back", "Back", dark, {
            let entity = entity.clone();
            move |_, _, cx| {
                entity.update(cx, |this, cx| {
                    this.close_history_selection(cx);
                });
            }
        }))
        .child(div().text_sm().text_color(muted).child(format!(
            "{} {}",
            entry.status_mark(),
            entry.title()
        )))
        .children(entry.receipt.as_ref().map(|receipt| {
            let pairs: Vec<(String, Option<PathBuf>)> = receipt
                .artifacts
                .iter()
                .map(|name| (name.clone(), entry.artifact_path(name)))
                .collect();
            render_receipt(receipt.actions.clone(), pairs, muted, dark)
        }))
        .children(entry.receipt.is_none().then(|| {
            div()
                .text_sm()
                .text_color(muted)
                .child(match entry.status.as_str() {
                    "failed" => "This task did not finish.",
                    "cancelled" => "This task was cancelled.",
                    "running" => "This task was interrupted.",
                    _ => "No receipt saved.",
                })
        }))
        .into_any_element()
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
    dark: bool,
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
        TranscriptBlock::Text { text } => markdown::render(
            text.trim_end(),
            MarkdownPalette::for_appearance(result_color, muted, dark),
            index,
        ),
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

    #[test]
    fn failed_provider_errors_offer_settings() {
        assert!(failed_offers_settings(
            crosspond_core::MISSING_API_KEY_MESSAGE
        ));
        assert!(failed_offers_settings(
            "Couldn’t connect to your AI provider. 401 Unauthorized"
        ));
        assert!(!failed_offers_settings("Agent step limit exceeded"));
    }
}
