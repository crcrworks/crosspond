# Crosspond

Command bar for your computer agent. macOS only.

Phase 13 adds a Chrome extension so Chromium pages use DOM snapshots and refs (`browser_snapshot` / `browser_click`) instead of Accessibility or screenshots. Native apps still use the Phase 11 Whole-Mac tools. Phase 12 receipts, history, and onboarding stay. Settings can sign in with ChatGPT Plus/Pro and keep several OpenAI Compatible endpoints; pick the model in the launcher.

## Run

```bash
# Computer use needs cua-driver on PATH (https://cua.ai/cua-driver).
# Override with CUA_DRIVER_BIN if it is installed somewhere else.
npm --prefix ui install
npm --prefix ui run desktop
```

That runs the Tauri 2 host (`crates/crosspond-app`) and the SvelteKit Vite server together. To run them separately: `npm --prefix ui run dev` then `cargo build -p crosspond-chrome-host && cargo run -p crosspond-app`. `npm --prefix ui run check` and `npm --prefix ui test` cover the frontend. `cargo build` at the workspace root also builds `crosspond-chrome-host`, which Chrome launches for native messaging.

Then:

1. On first launch, Crosspond opens a welcome. Press **⌘,** (or **Open Settings**) and open the **Models** tab: sign in with ChatGPT Plus/Pro and/or add one or more OpenAI Compatible endpoints (name, Base URL, API key). You can use both at once. Optionally add an Exa API key on the **Search** tab. The Knowledge Vault path defaults to `~/Documents/Crosspond` (**Knowledge** tab).
2. Click **Save** / **Test** on each provider, then **Continue**. On **Crosspond is ready**, click **Open** (or press the launcher shortcut) to show the command bar. Pick the model (and ChatGPT effort) under the launcher prompt. If port 1455 is already in use (often Codex CLI), paste the ChatGPT redirect URL to finish sign-in.
3. Grant Accessibility, Screen Recording, or Calendars later from Settings → Permissions, or when a tool needs them. Finder selection may also prompt for Automation.
4. Select text in another app, or files in Finder — or leave Safari / Chrome / Helium in front.
5. Press **Option + Space** (change this in Settings) to show the bar from any app. A badge should show the app and “Selected text: N chars” or “N selected files”.
6. Keep Crosspond running so it can register `com.crosspond.chrome`. Then load the unpacked extension from Settings → Browser (chrome://extensions → Developer mode → Load unpacked → `extension/chrome`). If Chrome already had the extension loaded, click Reload. Settings → Browser should show **Connected**. Then ask to click something on the current page; Crosspond uses `browser_snapshot` rather than screenshots.
7. Try **Summarize this**, **What shipped in Rust 1.96?** (needs Exa key), **カレンダーから今日の予定を確認して** (`calendar_events`), **Press the Continue button** in a native app, **PDF のスキル入れて** (`skill_search` / `skill_install` into `~/.crosspond/skills`; a host scan refuses malicious skills even in Auto), or ask to click something visible in a browser page. UI actions, shell, external files, and a new browser site show an **Allow** / **Cancel** card first unless the chip is **Auto**. Auto does not add new sites to Allowed Sites. After a task, the summary stays in the conversation and artifacts get **Show in Finder**; **History** opens a past conversation so you can follow up.

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
