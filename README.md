# Crosspond

Command bar for your computer agent. macOS only.

Phase 3 is BYOK chat, a task workspace, and ambient context: selected text and Finder files so you can say “this”.

## Run

Xcode 27 needs the Metal toolchain once:

```bash
xcodebuild -downloadComponent MetalToolchain
```

Then:

```bash
cargo run -p crosspond-app
```

Then:

1. Press **⌘,** (or Crosspond → Settings) and set Base URL, model, and API key.
2. Click **Save**, then **Test Connection**.
3. Grant Accessibility when macOS asks (needed for selected text). Finder selection may also prompt for Automation.
4. Select text in another app, or files in Finder.
5. Press **Option + Space**. A badge should show the app and “Selected text: N chars” or “N selected files”.
6. Type **Summarize this** and press Enter.

**Escape** or **Stop** cancels. Hiding the window starts a new session.

If hotkey registration fails, the window opens immediately so the app is still usable.

For local OpenAI-compatible servers, Base URL must include `/v1` (for example `http://127.0.0.1:1234/v1`). An empty API key is rejected; use a dummy value such as `lm-studio`.

## Develop

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Bundle notes

`resources/macos/Info.plist` is the minimum accessory-app manifest (`LSUIElement`). `cargo run` still shows a Dock icon because it is not wrapped in an `.app` bundle yet. Accessibility / Apple Events usage strings are in that plist for a future bundle.
