#![deny(unsafe_code)]

mod commands;
mod events;
mod launcher;
mod state;

use std::sync::Arc;

use crosspond_core::{FileConfigStore, spawn_runtime_with_tools};
use crosspond_macos::{
    MacOsContextCollector, MacOsGlobalHotkey, MacOsKeychainSecretStore, macos_agent_backends,
};
use crosspond_tools::computer_and_screenshot_registry;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Manager, RunEvent, WindowEvent};

use crate::state::{AppState, NoopHotkey};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = Arc::new(FileConfigStore::in_home());
    let secrets: Arc<dyn crosspond_core::SecretStore> = Arc::new(MacOsKeychainSecretStore);
    let (accessibility, screenshot, apps, input, calendar) = macos_agent_backends();
    let (channels, runtime) = spawn_runtime_with_tools(
        Arc::clone(&config) as _,
        Arc::clone(&secrets),
        Arc::new(computer_and_screenshot_registry(
            Arc::new(accessibility),
            Arc::new(screenshot),
            Arc::new(apps),
            Arc::new(input),
            Arc::new(calendar),
        )),
    );

    let (hotkey, start_visible) = match MacOsGlobalHotkey::register_default() {
        Ok(hotkey) => (Box::new(hotkey) as _, false),
        Err(err) => {
            eprintln!("crosspond: {err}; showing the launcher without a hotkey");
            (Box::new(NoopHotkey) as _, true)
        }
    };

    let app_state = AppState::new(
        channels.commands,
        config,
        secrets,
        Arc::new(MacOsContextCollector),
        hotkey,
        runtime,
    );
    let events = channels.events;

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::start_task,
            commands::approve,
            commands::reject,
            commands::cancel,
            commands::reset_session,
            commands::hide_launcher,
            commands::open_settings,
            commands::load_settings,
            commands::save_config,
            commands::save_secret,
            commands::test_connection,
            commands::list_history,
            commands::cycle_computer_approval,
            commands::permissions,
            commands::open_system_settings,
            commands::reveal_artifact,
            commands::reveal_history_artifact,
            commands::set_ui_flags,
            commands::sync_launcher_size,
        ])
        .setup(move |app| {
            install_menu(app.handle())?;
            events::start_event_loop(app.handle().clone(), events);
            launcher::start_hotkey_loop(app.handle().clone());

            if let Some(window) = launcher::launcher_window(app.handle()) {
                launcher::apply_transparency(&window);
                launcher::position_launcher(&window);
            }

            let needs_onboarding =
                !crosspond_core::provider_key_is_set(&*app.state::<AppState>().secrets);
            if start_visible || needs_onboarding {
                launcher::show(app.handle());
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "launcher" && matches!(event, WindowEvent::Focused(false)) {
                launcher::hide_compact_if_unfocused(window.app_handle());
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Crosspond")
        .run(|app, event| {
            if let RunEvent::Reopen { .. } = event {
                launcher::show(app);
            }
        });
}

fn install_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let quit = PredefinedMenuItem::quit(app, Some("Quit Crosspond"))?;
    let menu = Menu::with_items(
        app,
        &[&Submenu::with_items(
            app,
            "Crosspond",
            true,
            &[&settings, &PredefinedMenuItem::separator(app)?, &quit],
        )?],
    )?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        if event.id() == "settings" {
            let _ = commands::open_settings(app.clone());
        }
    });
    Ok(())
}
