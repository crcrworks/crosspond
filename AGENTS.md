# Crosspond agent notes

Treat `docs/mvp.md` and `docs/architecture.md` as the product and design source of truth.

## Phase rule

Implement one phase at a time. Do not start the next phase until the current one builds, formats, lints, and tests.

Current phase: **4 — Accessibility computer use + approvals**.

Out of scope until later phases: screenshots, history, onboarding polish.

## Crate boundaries

```
crosspond-app
      ↓
crosspond-core
 ↙          ↘
model       tools ← macos
```

- Only `crosspond-app` may depend on GPUI.
- `crosspond-core`, `crosspond-model`, and `crosspond-tools` must not depend on GPUI.
- `crosspond-model` must not depend on `crosspond-core`.
- `crosspond-tools` must not depend on `crosspond-macos` (that would cycle through core).
- `crosspond-macos` may depend on `crosspond-tools` and implements `AccessibilityBackend`.
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

Never persist API keys in `config.json`, `.env`, SQLite, logs, or task history. Store them in Keychain via `SecretStore`. `SecretString` must not derive `Debug`. Do not log selected text, Finder paths, or Accessibility field values (especially passwords).
