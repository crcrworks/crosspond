#![deny(unsafe_code)]

mod actions;
mod activity_label;
mod assets;
mod command_window;
mod launcher;
mod markdown;
mod settings;
mod text_input;
mod transcript;
mod ui;

use std::sync::Arc;

use crosspond_core::{ConfigStore, FileConfigStore, SecretStore, spawn_runtime_with_tools};
use crosspond_macos::{
    MacOsContextCollector, MacOsGlobalHotkey, MacOsKeychainSecretStore, macos_agent_backends,
};
use crosspond_tools::computer_and_screenshot_registry;
use gpui::{App, Application, Menu, MenuItem, SystemMenuType, prelude::*};

use command_window::CommandWindow;

fn main() {
    let config: Arc<dyn ConfigStore> = Arc::new(FileConfigStore::in_home());
    let secrets: Arc<dyn SecretStore> = Arc::new(MacOsKeychainSecretStore);
    let (accessibility, screenshot, apps, input, calendar) = macos_agent_backends();
    let (channels, _runtime) = spawn_runtime_with_tools(
        Arc::clone(&config),
        Arc::clone(&secrets),
        Arc::new(computer_and_screenshot_registry(
            Arc::new(accessibility),
            Arc::new(screenshot),
            Arc::new(apps),
            Arc::new(input),
            Arc::new(calendar),
        )),
    );
    let command_tx = channels.commands;
    let event_rx = channels.events;

    let application = Application::new().with_assets(assets::Assets);
    application.on_reopen(|cx| {
        launcher::show(cx);
    });
    application.run(move |cx: &mut App| {
        settings::install_services(
            settings::Services {
                commands: command_tx.clone(),
                config: Arc::clone(&config),
                secrets,
            },
            cx,
        );

        let mut bindings = text_input::key_bindings();
        bindings.extend(command_window::key_bindings());
        bindings.extend(actions::key_bindings());
        cx.bind_keys(bindings);
        cx.on_action(|_: &actions::Quit, cx| cx.quit());
        cx.on_action(|_: &actions::OpenSettings, cx| settings::open(cx));
        cx.set_menus(vec![Menu {
            name: "Crosspond".into(),
            items: vec![
                MenuItem::action("Settings…", actions::OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Crosspond", actions::Quit),
            ],
        }]);

        let (hotkey, start_visible) = match MacOsGlobalHotkey::register_default() {
            Ok(hotkey) => (Box::new(hotkey) as _, false),
            Err(err) => {
                eprintln!("crosspond: {err}; showing the launcher without a hotkey");
                (launcher::noop_hotkey(), true)
            }
        };

        let window = cx
            .open_window(launcher::window_options(cx), {
                let command_tx = command_tx.clone();
                let config = Arc::clone(&config);
                move |window, cx| {
                    cx.new(|cx| {
                        let view = CommandWindow::new(command_tx, config, cx);
                        let focus = view.input_focus_handle(cx);
                        window.focus(&focus);
                        view
                    })
                }
            })
            .expect("failed to open command window");

        launcher::install(
            window,
            event_rx,
            hotkey,
            Arc::new(MacOsContextCollector),
            cx,
        );

        if start_visible || settings::needs_onboarding(cx) {
            launcher::show(cx);
        }
    });
}
