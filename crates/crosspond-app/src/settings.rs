use std::sync::Arc;

use crosspond_core::{
    AgentEvent, AppConfig, CommandSender, ConfigStore, RuntimeCommand, SecretKey, SecretStore,
    SecretString,
};
use gpui::{
    App, Bounds, Context, Entity, FocusHandle, FontWeight, Global, TitlebarOptions, Window,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, div, prelude::*, px, rgb, size,
};

use crate::text_input::TextInput;
use crate::ui;

pub struct Services {
    pub commands: CommandSender,
    pub config: Arc<dyn ConfigStore>,
    pub secrets: Arc<dyn SecretStore>,
}

impl Global for Services {}

struct SettingsHost {
    window: Option<WindowHandle<SettingsWindow>>,
}

impl Global for SettingsHost {}

pub fn install_services(services: Services, cx: &mut App) {
    cx.set_global(services);
    cx.set_global(SettingsHost { window: None });
}

pub fn open(cx: &mut App) {
    if !cx.has_global::<Services>() {
        return;
    }
    if let Some(handle) = cx.global::<SettingsHost>().window
        && handle
            .update(cx, |_, window, _| {
                window.activate_window();
            })
            .is_ok()
    {
        cx.activate(true);
        return;
    }

    let (commands, config, secrets) = {
        let services = cx.global::<Services>();
        (
            services.commands.clone(),
            Arc::clone(&services.config),
            Arc::clone(&services.secrets),
        )
    };

    let handle = cx
        .open_window(window_options(cx), {
            let commands = commands.clone();
            let config = Arc::clone(&config);
            let secrets = Arc::clone(&secrets);
            move |window, cx| {
                cx.new(|cx| {
                    let view = SettingsWindow::new(commands, config, secrets, cx);
                    let focus = view.first_field_focus(cx);
                    window.focus(&focus);
                    view
                })
            }
        })
        .expect("failed to open settings");

    cx.global_mut::<SettingsHost>().window = Some(handle);
    cx.activate(true);
}

pub fn apply_event(event: &AgentEvent, cx: &mut App) -> bool {
    let AgentEvent::ConnectionTested { ok, message } = event else {
        return false;
    };
    let Some(handle) = cx.global::<SettingsHost>().window else {
        return true;
    };
    let ok = *ok;
    let message = message.clone();
    let _ = handle.update(cx, |view, _, cx| {
        view.set_test_result(ok, message, cx);
    });
    true
}

fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(480.), px(560.)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Settings".into()),
            ..Default::default()
        }),
        kind: WindowKind::Normal,
        is_resizable: true,
        app_id: Some("com.crosspond.app".into()),
        window_min_size: Some(size(px(400.), px(420.))),
        ..Default::default()
    }
}

pub struct SettingsWindow {
    base_url: Entity<TextInput>,
    model: Entity<TextInput>,
    api_key: Entity<TextInput>,
    key_already_stored: bool,
    test_status: Option<(bool, String)>,
    save_status: Option<String>,
    commands: CommandSender,
    config: Arc<dyn ConfigStore>,
    secrets: Arc<dyn SecretStore>,
}

impl SettingsWindow {
    fn new(
        commands: CommandSender,
        config: Arc<dyn ConfigStore>,
        secrets: Arc<dyn SecretStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let loaded = config.load().unwrap_or_default();
        let key_already_stored = secrets
            .get(&SecretKey::PROVIDER_API_KEY)
            .ok()
            .flatten()
            .is_some_and(|key| !key.is_empty());

        let base_url = cx.new(|cx| {
            let mut input = TextInput::new("https://api.openai.com/v1", cx);
            input.set_text(loaded.base_url.clone());
            input
        });
        let model = cx.new(|cx| {
            let mut input = TextInput::new("gpt-4o-mini", cx);
            input.set_text(loaded.model.clone());
            input
        });
        let api_key = cx.new(|cx| {
            let placeholder = if key_already_stored {
                "••••••••  stored in Keychain"
            } else {
                "Required — stored in Keychain"
            };
            TextInput::new(placeholder, cx)
        });

        Self {
            base_url,
            model,
            api_key,
            key_already_stored,
            test_status: None,
            save_status: None,
            commands,
            config,
            secrets,
        }
    }

    fn first_field_focus(&self, cx: &App) -> FocusHandle {
        self.base_url.read(cx).focus_handle_clone()
    }

    fn set_test_result(&mut self, ok: bool, message: String, cx: &mut Context<Self>) {
        self.test_status = Some((ok, message));
        cx.notify();
    }

    fn persist(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let mut config = self.config.load().unwrap_or_else(|_| AppConfig::default());
        let base_url = self.base_url.read(cx).text().trim().to_string();
        let model = self.model.read(cx).text().trim().to_string();
        let api_key = self.api_key.read(cx).text().trim().to_string();
        let defaults = AppConfig::default();
        config.provider = defaults.provider;
        config.base_url = if base_url.is_empty() {
            defaults.base_url
        } else {
            base_url
        };
        config.model = if model.is_empty() {
            defaults.model
        } else {
            model
        };
        self.config.save(&config).map_err(|err| err.to_string())?;
        if !api_key.is_empty() {
            self.secrets
                .set(&SecretKey::PROVIDER_API_KEY, &SecretString::new(api_key))
                .map_err(|err| err.to_string())?;
            self.key_already_stored = true;
            self.api_key.update(cx, |input, cx| {
                input.reset();
                input.set_placeholder("••••••••  stored in Keychain");
                cx.notify();
            });
        }
        Ok(())
    }

    fn on_save(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        match self.persist(cx) {
            Ok(()) => {
                self.save_status = Some("Saved.".into());
                self.test_status = None;
            }
            Err(message) => {
                self.save_status = None;
                self.test_status = Some((false, message));
            }
        }
        cx.notify();
    }

    fn on_test(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Err(message) = self.persist(cx) {
            self.test_status = Some((false, message));
            cx.notify();
            return;
        }
        self.save_status = Some("Saved.".into());
        self.test_status = Some((true, "Testing connection…".into()));
        self.commands.send(RuntimeCommand::TestConnection);
        cx.notify();
    }
}

impl gpui::Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = ui::is_dark(window);
        let bg = if dark { rgb(0x1c1c1e) } else { rgb(0xffffff) };
        let text = if dark { rgb(0xf5f5f7) } else { rgb(0x1d1d1f) };
        let muted = if dark { rgb(0x8e8e93) } else { rgb(0x6e6e73) };
        let border = if dark { rgb(0x3a3a3c) } else { rgb(0xd2d2d7) };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            .text_color(text)
            .child(
                div()
                    .id("settings")
                    .flex()
                    .flex_col()
                    .size_full()
                    .overflow_y_scroll()
                    .px_6()
                    .py_4()
                    .gap_4()
                    .child(section_label("General", muted))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(field_label("Launch Crosspond", muted))
                            .child(div().child("Option + Space")),
                    )
                    .child(section_label("AI", muted))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(field_label("Provider", muted))
                            .child(div().child("OpenAI Compatible")),
                    )
                    .child(labeled_field(
                        "Base URL",
                        muted,
                        self.base_url.clone(),
                        border,
                    ))
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .whitespace_normal()
                            .child("Must include /v1, e.g. http://127.0.0.1:1234/v1"),
                    )
                    .child(labeled_field("Model", muted, self.model.clone(), border))
                    .child(labeled_field(
                        "API Key",
                        muted,
                        self.api_key.clone(),
                        border,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(ui::button("save", "Save", dark, {
                                let entity = cx.entity();
                                move |event, window, cx| {
                                    entity.update(cx, |this, cx| this.on_save(event, window, cx));
                                }
                            }))
                            .child(ui::button("test", "Test Connection", dark, {
                                let entity = cx.entity();
                                move |event, window, cx| {
                                    entity.update(cx, |this, cx| this.on_test(event, window, cx));
                                }
                            })),
                    )
                    .children(self.save_status.clone().map(|line| {
                        div()
                            .text_sm()
                            .text_color(muted)
                            .whitespace_normal()
                            .child(line)
                    }))
                    .children(self.test_status.clone().map(|(ok, message)| {
                        let color = if ok { rgb(0x30d158) } else { rgb(0xff453a) };
                        div()
                            .text_sm()
                            .text_color(color)
                            .whitespace_normal()
                            .child(message)
                    })),
            )
    }
}

fn section_label(label: &'static str, color: gpui::Rgba) -> impl IntoElement {
    div()
        .pt_2()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(label)
}

fn field_label(label: &'static str, color: gpui::Rgba) -> impl IntoElement {
    div().text_sm().text_color(color).child(label)
}

fn labeled_field(
    label: &'static str,
    muted: gpui::Rgba,
    input: Entity<TextInput>,
    border: gpui::Rgba,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(field_label(label, muted))
        .child(
            div()
                .w_full()
                .px_2()
                .py_1()
                .rounded_sm()
                .border_1()
                .border_color(border)
                .child(input),
        )
}
