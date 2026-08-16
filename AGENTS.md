# Crosspond agent notes

Treat `docs/mvp.md` and `docs/architecture.md` as the product and design source of truth.

## Phase rule

Implement one phase at a time. Do not start the next phase until the current one builds, formats, lints, and tests.

Current phase: **Knowledge Vault 1 — Vault foundation**.

Out of scope until later phases: Knowledge Vault Phases 2–8, drag, `kill_app`, exposing cua-driver’s full catalog, signing/notarization.

## Crate boundaries

```
crosspond-app
       ↓
crosspond-core
   ↙       ↘
model    knowledge
  │
  └── tools ← macos
```

- Only `crosspond-app` may depend on GPUI.
- `crosspond-core`, `crosspond-model`, `crosspond-tools`, and `crosspond-knowledge` must not depend on GPUI.
- `crosspond-model` must not depend on `crosspond-core`.
- `crosspond-knowledge` must not depend on GPUI, `crosspond-core`, or `crosspond-macos`.
- `crosspond-tools` must not depend on `crosspond-macos` (that would cycle through core).
- `crosspond-macos` may depend on `crosspond-tools` and implements `AccessibilityBackend`, `ScreenshotBackend`, `AppBackend`, `InputBackend`, and `CalendarBackend` (cua-driver + EventKit).
- UI and core must not import `global-hotkey` or Security framework crates directly.
- `unsafe` stays in `crosspond-macos` (or platform bindings it wraps) and needs a comment.

## GPUI

Do not guess GPUI APIs. Confirm against the pinned crate version:

- `gpui = "=0.2.2"` (crates.io, git revision `69e2130295c2649963eb639fc70b4f2ee8ea1624`)
- that version's `examples/`
- compiler errors

Do not copy later Zed `main` examples (`gpui_platform::application`, extra `ShapedLine::paint` arguments, etc.).

GPUI 0.2.2 has no per-window hide. `App::hide()` / `App::activate(true)` hide and show the whole app, including Settings.

## Secrets

Never persist API keys in `config.json`, `.env`, SQLite, logs, task history, or the Knowledge Vault. Store them in Keychain via `SecretStore`. `SecretString` must not derive `Debug`. Do not log selected text, Finder paths, Accessibility field values (especially passwords), screenshot bytes, or calendar event notes/bodies. Vault notes may only store `credential_ref` pointers, never secret values.
