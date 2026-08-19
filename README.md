# Crosspond

Command bar for your computer agent. macOS only.

Phase 12 adds receipts in the launcher, recent task history, and first-launch onboarding on top of the Phase 11 Whole-Mac agent: retarget any app (`list_apps` / `open_app`), keyboard and scroll, `run_command` / `open_url` (Allow in Manual/AI; Auto runs them), EventKit `calendar_events`, and external file reads — plus earlier BYOK chat, workspace tools, ambient context, Accessibility, and screenshot/click.

## Run

```bash
# Computer use needs cua-driver on PATH (https://cua.ai/cua-driver).
# Override with CUA_DRIVER_BIN if it is installed somewhere else.
npm --prefix ui install
npm --prefix ui run desktop
```

That runs the Tauri 2 host (`crates/crosspond-app`) and the SvelteKit Vite server together. To run them separately: `npm --prefix ui run dev` then `cargo run -p crosspond-app`. `npm --prefix ui run check` and `npm --prefix ui test` cover the frontend.

Then:

1. On first launch, Crosspond opens a welcome. Press **⌘,** (or **Open Settings**) and set Base URL, model, and API key. Optionally add an Exa API key for `web_search`. The Knowledge Vault path defaults to `~/Documents/Crosspond`; change it in Settings if you want a different folder.
2. Click **Save**, then **Test Connection**, then **Continue**.
3. Grant Accessibility, Screen Recording, or Calendars later from Settings → Permissions, or when a tool needs them. Finder selection may also prompt for Automation.
4. Select text in another app, or files in Finder — or leave Safari / Helium in front.
5. Press **Option + Space** (change this in Settings). A badge should show the app and “Selected text: N chars” or “N selected files”.
6. Try **Summarize this**, **What shipped in Rust 1.96?** (needs Exa key), **カレンダーから今日の予定を確認して** (`calendar_events`), **Press the Continue button** in Safari, or ask to click something visible in a browser page. UI actions, shell, and external files show an **Allow** / **Cancel** card first unless the chip is **Auto**. After a task, the summary stays in the conversation and artifacts get **Show in Finder**; **History** opens a past conversation so you can follow up.

**Escape** or **Stop** cancels the whole task while it is running. Approval **Cancel** rejects only that action. Hiding the window keeps work running and keeps the conversation; press **New** to start fresh.

If hotkey registration fails, the window opens immediately so the app is still usable.

For local OpenAI-compatible servers, Base URL must include `/v1` (for example `http://127.0.0.1:1234/v1`). An empty API key is rejected; use a dummy value such as `lm-studio`. Vision-capable models are required for screenshot tools.

## Develop

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix ui run check
npm --prefix ui test
```

## Bundle notes

`resources/macos/Info.plist` is the minimum accessory-app manifest (`LSUIElement`). The Tauri host also sets Accessory at runtime so `cargo run` / `tauri dev` do not steal the frontmost app (tao defaults to Regular). Accessibility / Apple Events usage strings are in that plist for a future bundle.
