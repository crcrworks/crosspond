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

The WebView must never receive API keys, selected text, Finder paths, screenshot bytes, or calendar notes. Ambient UI gets `badge_lines()` only. `ContextCapsule` stays in Rust `AppState`.

Hide and show the launcher with per-window `hide()` / `show()`. Do not hide the whole app.

WKWebView owns Japanese IME. Compact-bar click-away still skips hide while the app is active (IME candidate windows).

## Secrets

Never persist API keys in `config.json`, `.env`, SQLite, logs, task history, or the Knowledge Vault. Store them in Keychain via `SecretStore`. `SecretString` must not derive `Debug`. Do not log selected text, Finder paths, Accessibility field values (especially passwords), screenshot bytes, or calendar event notes/bodies. Vault notes may only store `credential_ref` pointers, never secret values.
