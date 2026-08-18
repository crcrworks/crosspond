# Crosspond agent notes

Treat `docs/mvp.md` and `docs/architecture.md` as the product and design source of truth.

## Phase rule

Implement one phase at a time. Do not start the next phase until the current one builds, formats, lints, and tests.

Current phase: **12 — Polish (receipts UI, history, onboarding)**.

Out of scope until later phases: drag, `kill_app`, exposing cua-driver’s full catalog, signing/notarization.

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

Never persist API keys in `config.json`, `.env`, SQLite, logs, or task history. Store them in Keychain via `SecretStore`. `SecretString` must not derive `Debug`. Do not log selected text, Finder paths, Accessibility field values (especially passwords), screenshot bytes, or calendar event notes/bodies.

## Cursor Cloud specific instructions

Crosspond is a **macOS-only** GPUI product, but the Cloud Agent VM is **headless Linux**. Standard dev commands live in `README.md` (`cargo fmt`, `cargo clippy`, `cargo test`, `cargo run -p crosspond-app`); the notes below only cover Linux-specific caveats. Rust `1.96.0` (with `rustfmt`/`clippy`) is pinned by `rust-toolchain.toml` and preinstalled.

What works on Linux: `cargo build --workspace`, `cargo test --workspace` (127 tests pass), and `cargo fmt --all --check`. What does not: **running the GUI** — `cargo run -p crosspond-app` builds and starts, then GPUI panics at `platform.rs` (`Failed to initialize X11 client` / `NoSupportedDeviceFound`) because there is no display or Vulkan GPU. That is expected; the real app needs macOS + Metal + a display, so validate GUI changes on macOS.

- **App linking:** GPUI links `libstdc++`, but Rust's bundled `lld` does not search the gcc dir (`/usr/lib/gcc/x86_64-linux-gnu/13`) where `libstdc++.so` lives, so `crosspond-app` fails to link out of the box. This is fixed by a Linux-only global cargo config at `/usr/local/cargo/config.toml` (outside the repo, baked into the VM snapshot). Do **not** set a `RUSTFLAGS` env var — it overrides that config and app linking breaks again.
- **Clippy caveat:** the documented `cargo clippy --workspace --all-targets -- -D warnings` is clean on macOS but fails on Linux **only inside `crosspond-macos`** stub code (`needless_return` / unused imports that don't exist on the macOS implementations). On Linux, lint the platform-independent crates instead: `cargo clippy -p crosspond-core -p crosspond-model -p crosspond-tools --all-targets -- -D warnings`.
- **macOS-gated tests:** `crosspond-macos` runs only 2 unit tests on Linux; the rest are `#[cfg(target_os = "macos")]`.
- **Running the core headlessly:** the agent engine `crosspond-app/src/main.rs` drives (`spawn_runtime_with_tools`) is platform-independent and fully runnable without macOS. `crosspond-core`'s runtime tests (e.g. `tool_loop_writes_workspace_file` in `src/runtime.rs`) exercise `StartTask` → model stream → tool call → `receipt.json` end-to-end and are the best way to validate core behavior on Linux.
