#![deny(unsafe_code)]

mod browser;
mod commands;
mod events;
mod launcher;
mod navigation;
mod oauth;
mod state;

use std::sync::Arc;

use crosspond_core::{ConfigStore, FileConfigStore, GlobalHotkeyService, spawn_runtime_with_tools};
use crosspond_macos::{
    MacOsContextCollector, MacOsGlobalHotkey, MacOsKeychainSecretStore, macos_agent_backends,
};
use crosspond_tools::computer_and_screenshot_registry_with_browser;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Manager, RunEvent, WindowEvent};

use crate::browser::start_browser_backend;
use crate::state::{AppState, NoopHotkey};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = Arc::new(FileConfigStore::in_home());
    let secrets: Arc<dyn crosspond_core::SecretStore> = Arc::new(MacOsKeychainSecretStore);
    let (accessibility, screenshot, apps, input, calendar) = macos_agent_backends();
    let (browser_bridge, browser_backend) = start_browser_backend();
    let (channels, runtime) = spawn_runtime_with_tools(
        Arc::clone(&config) as _,
        Arc::clone(&secrets),
        Arc::new(computer_and_screenshot_registry_with_browser(
            Arc::new(accessibility),
            Arc::new(screenshot),
            Arc::new(apps),
            Arc::new(input),
            Arc::new(calendar),
            browser_backend,
        )),
    );

    let app_config = config.load().unwrap_or_default();
    let (hotkey, start_visible) = match MacOsGlobalHotkey::new() {
        Ok(mut hotkey) => match hotkey.set_hotkey(&app_config.launcher_hotkey) {
            Ok(()) => (Box::new(hotkey) as _, false),
            Err(err) => {
                eprintln!("crosspond: {err}; showing the launcher without a hotkey");
                (Box::new(hotkey) as _, true)
            }
        },
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
        browser_bridge,
    );
    let events = channels.events;

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(external_navigation_plugin())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::start_task,
            commands::approve,
            commands::reject,
            commands::submit_credential,
            commands::cancel,
            commands::reset_session,
            commands::hide_launcher,
            commands::open_settings,
            commands::load_settings,
            commands::save_config,
            commands::save_compat,
            commands::add_compat,
            commands::delete_compat,
            commands::save_selected,
            commands::save_effort,
            commands::list_models,
            commands::set_launcher_hotkey,
            commands::pause_launcher_hotkey,
            commands::resume_launcher_hotkey,
            commands::save_secret,
            commands::start_chatgpt_login,
            commands::complete_chatgpt_login,
            commands::cancel_chatgpt_login,
            commands::sign_out_chatgpt,
            commands::test_connection,
            commands::test_compat_connection,
            commands::list_history,
            commands::open_conversation,
            commands::cycle_computer_approval,
            commands::set_computer_approval,
            commands::permissions,
            commands::open_system_settings,
            commands::reveal_artifact,
            commands::reveal_history_artifact,
            commands::set_ui_flags,
            commands::sync_launcher_size,
            commands::list_mention_apps,
            commands::open_external_url,
        ])
        .setup(move |app| {
            // tao defaults to Regular, which overrides Info.plist LSUIElement and
            // makes Crosspond the frontmost app. Computer tools then see only us.
            #[cfg(target_os = "macos")]
            let _ = app
                .handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory);

            install_menu(app.handle())?;
            events::start_event_loop(app.handle().clone(), events);
            launcher::start_hotkey_loop(app.handle().clone());

            if let Some(window) = launcher::launcher_window(app.handle()) {
                launcher::apply_transparency(&window);
                launcher::position_launcher(&window);
            }

            let state = app.state::<AppState>();
            let needs_onboarding = !crosspond_core::provider_is_ready(
                &state.config.load().unwrap_or_default(),
                &*state.secrets,
            );
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

fn external_navigation_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("external-nav")
        .on_navigation(|_webview, url| navigation::handle_navigation(url))
        .build()
}

fn install_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let quit = PredefinedMenuItem::quit(app, Some("Quit Crosspond"))?;
    // macOS routes ⌘C / ⌘V / ⌘X / ⌘A / ⌘Z through the Edit menu even for
    // accessory apps whose menu bar is not shown. Without these items,
    // WKWebView cannot copy or paste in the prompt or Settings fields.
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                "Crosspond",
                true,
                &[&settings, &PredefinedMenuItem::separator(app)?, &quit],
            )?,
            &edit,
        ],
    )?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        if event.id() == "settings" {
            let _ = commands::open_settings(app.clone());
        }
    });
    Ok(())
}
