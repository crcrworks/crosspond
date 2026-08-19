# Crosspond agent notes

Treat `docs/mvp.md` and `docs/architecture.md` as the product and design source of truth.

## Phase rule

Implement one phase at a time. Do not start the next phase until the current one builds, formats, lints, and tests.

Current phase: **Knowledge Vault complete (Phases 0–8)**. UI host is Tauri 2 + SvelteKit.

Out of scope until later: drag, `kill_app`, exposing cua-driver’s full catalog, signing/notarization.

## Crate boundaries

```
crosspond-app (Tauri)    ui/ (SvelteKit SPA)
       ↓
crosspond-core
   ↙       ↘
model    knowledge
  │
  └── tools ← macos
```

- Only `crosspond-app` may depend on Tauri. Svelte lives in `ui/` and talks to Rust only through invoke/events.
- `crosspond-core`, `crosspond-model`, `crosspond-tools`, and `crosspond-knowledge` must not depend on Tauri or Svelte.
- `crosspond-model` must not depend on `crosspond-core`.
- `crosspond-knowledge` must not depend on Tauri, `crosspond-core`, or `crosspond-macos`.
- `crosspond-tools` must not depend on `crosspond-macos` (that would cycle through core).
- `crosspond-macos` may depend on `crosspond-tools` and implements `AccessibilityBackend`, `ScreenshotBackend`, `AppBackend`, `InputBackend`, and `CalendarBackend` (cua-driver + EventKit).
- UI and core must not import `global-hotkey` or Security framework crates directly.
- `unsafe` stays in `crosspond-macos` (or platform bindings it wraps) and needs a comment.

## Tauri / Svelte

Do not guess Tauri 2 APIs. Confirm against the pinned crate versions in the workspace `Cargo.toml`, `crates/crosspond-app/tauri.conf.json`, and compiler errors.

The frontend is Svelte 5 + SvelteKit 2 in SPA mode (`@sveltejs/adapter-static` with `fallback: 'index.html'`, `ssr = false`). Do not add SvelteKit server routes or `load` functions that must run at build time against Tauri APIs.

The WebView must never receive API keys, ChatGPT OAuth tokens, JWTs, account ids, selected text, Finder paths, screenshot bytes, or calendar notes. Ambient UI gets `badge_lines()` only. Settings may show `chatgpt_signed_in: bool`. `ContextCapsule` stays in Rust `AppState`.

Hide and show the launcher with per-window `hide()` / `show()`. Do not hide the whole app.

WKWebView owns Japanese IME. Compact-bar click-away still skips hide while the app is active (IME candidate windows).

## Secrets

Never persist API keys or ChatGPT OAuth tokens in `config.json`, `.env`, SQLite, logs, task history, or the Knowledge Vault. Store them in Keychain via `SecretStore` (`provider.api_key`, `exa.api_key`, `provider.chatgpt_oauth`). `SecretString` must not derive `Debug`. Do not log selected text, Finder paths, Accessibility field values (especially passwords), screenshot bytes, calendar event notes/bodies, or ChatGPT tokens/JWTs. Vault notes may only store `credential_ref` pointers, never secret values.

## Cursor Cloud specific instructions

Crosspond targets **macOS**, but the Cloud Agent VM is **headless Linux**. Standard dev commands live in `README.md`; the notes below only cover Linux-specific caveats. Rust `1.96.0` (with `rustfmt`/`clippy`) is pinned by `rust-toolchain.toml` and preinstalled.

The portable crates — `crosspond-core`, `crosspond-model`, `crosspond-tools`, and `crosspond-knowledge` — build, lint, and test on Linux. `cargo fmt --all --check` is clean.

- **Clippy caveat:** the documented `cargo clippy --workspace --all-targets -- -D warnings` is clean on macOS but fails on Linux **only inside `crosspond-macos`** stub code (`needless_return` / unused imports that don't exist on the macOS implementations). On Linux, lint the portable crates instead: `cargo clippy -p crosspond-core -p crosspond-model -p crosspond-tools -p crosspond-knowledge --all-targets -- -D warnings`.
- **macOS-gated tests:** `crosspond-macos` runs only 2 unit tests on Linux; the rest are `#[cfg(target_os = "macos")]` (cua-driver, EventKit, Keychain, hotkeys).
- **Running the core headlessly:** the agent engine `crosspond-app` drives (`spawn_runtime_with_tools`) is platform-independent and runnable without macOS; `crosspond-core`'s runtime tests exercise `StartTask` → model stream → tool call → receipt end-to-end and are the best way to validate core behavior on Linux.
- **GUI (Tauri):** `crosspond-app` is a **Tauri 2 + SvelteKit** app (frontend in `ui/`). The desktop GUI does not run on this headless VM, and building it on Linux needs WebKitGTK/GTK/libsoup3 system packages plus a `ui/` npm build that are **not** part of the current VM setup — validate GUI changes on macOS. NOTE: the Linux system-dependency setup and update script for this environment predate the GPUI→Tauri migration and should be re-validated for Tauri before relying on a Linux `crosspond-app` build.
