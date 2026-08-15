# Crosspond

Command bar for your computer agent. macOS only.

Phase 11 completes the Whole-Mac agent stack on top of Phase 6: retarget any app (`list_apps` / `open_app`), keyboard and scroll, approved `run_command` / `open_url`, EventKit `calendar_events`, and approved external file reads — plus earlier BYOK chat, workspace tools, ambient context, Accessibility, and screenshot/click.

## Run

Xcode 27 needs the Metal toolchain once:

```bash
xcodebuild -downloadComponent MetalToolchain
```

Then:

```bash
# Computer use needs cua-driver on PATH (https://cua.ai/cua-driver).
# Override with CUA_DRIVER_BIN if it is installed somewhere else.
cargo run -p crosspond-app
```

Then:

1. Press **⌘,** (or Crosspond → Settings) and set Base URL, model, and API key. Optionally add an Exa API key for `web_search`.
2. Click **Save**, then **Test Connection**.
3. Grant Accessibility when macOS asks (needed for selected text and UI actions). Grant Screen Recording for screenshots and clicks. Grant Calendars for `calendar_events`. Finder selection may also prompt for Automation.
4. Select text in another app, or files in Finder — or leave Safari / Helium in front.
5. Press **Option + Space**. A badge should show the app and “Selected text: N chars” or “N selected files”.
6. Try **Summarize this**, **What shipped in Rust 1.96?** (needs Exa key), **カレンダーから今日の予定を確認して** (`calendar_events`), **Press the Continue button** in Safari, or ask to click something visible in a browser page. UI actions show an **Allow** / **Cancel** card first (unless Auto).

**Escape** or **Stop** cancels the whole task. Approval **Cancel** rejects only that action. Hiding the window starts a new session.

If hotkey registration fails, the window opens immediately so the app is still usable.

For local OpenAI-compatible servers, Base URL must include `/v1` (for example `http://127.0.0.1:1234/v1`). An empty API key is rejected; use a dummy value such as `lm-studio`. Vision-capable models are required for screenshot tools.

## Develop

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Bundle notes

`resources/macos/Info.plist` is the minimum accessory-app manifest (`LSUIElement`). `cargo run` still shows a Dock icon because it is not wrapped in an `.app` bundle yet. Accessibility / Apple Events usage strings are in that plist for a future bundle.
