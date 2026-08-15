# Crosspond

Command bar for your computer agent. macOS only.

Phase 5 is BYOK chat, a task workspace, ambient context (“this”), Accessibility computer use with per-action approval, and window screenshots with approved image-coordinate clicks.

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

1. Press **⌘,** (or Crosspond → Settings) and set Base URL, model, and API key.
2. Click **Save**, then **Test Connection**.
3. Grant Accessibility when macOS asks (needed for selected text and UI actions). Grant Screen Recording for screenshots and clicks. Finder selection may also prompt for Automation.
4. Select text in another app, or files in Finder — or leave Safari / Helium in front.
5. Press **Option + Space**. A badge should show the app and “Selected text: N chars” or “N selected files”.
6. Try **Summarize this**, **Press the Continue button** in Safari, or ask to click something visible in a browser page. UI actions show an **Allow** / **Cancel** card first.

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
