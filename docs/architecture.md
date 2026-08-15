# Architecture

## Crates

| Crate | Role | May depend on |
| --- | --- | --- |
| `crosspond-app` | GPUI UI, process entry | core, macos, tools, GPUI |
| `crosspond-core` | runtime commands/events, agent loop, policy, receipts, context types | model, tools, tokio, uuid |
| `crosspond-model` | LLM provider abstraction | reqwest, serde |
| `crosspond-tools` | filesystem tools, computer-tool defs, `AccessibilityBackend` | serde; not macos |
| `crosspond-macos` | hotkeys, Keychain, ambient context, AX tree + actions | core, tools, platform crates, not GPUI |

`crosspond-model` must not depend on `crosspond-core`. `crosspond-tools` must not depend on `crosspond-macos` (core → tools and macos → core would cycle). macOS implements `AccessibilityBackend` from tools.

## Phase 4 data flow

```
GPUI thread                          Tokio runtime thread
───────────                          ────────────────────
Option+Space
        │
 collect context  (before activate)
        │
    show launcher + badges
        │
 Enter / StartTask(+capsule) ──mpsc──►  stage Finder files into input/
        │                      inject ambient block into system prompt
        │                      OpenAI-compatible stream
        │                      fs tools (workspace auto; external after Allow)
        │                      get_accessibility_snapshot (auto)
        │                      ui_press / ui_set_value ──► ApprovalRequired
 Allow / Cancel  ──mpsc──►     Approve / Reject that action
 Escape / Stop   ──mpsc──►     Cancel the whole task
 AgentEvent      ◄─mpsc──   ContextCollected / AssistantDelta /
        │                      ToolStarted / ApprovalRequired /
        │                      ArtifactCreated / completed
        ▼
 Command window / Settings
```

Commands and events are defined in `crosspond-core`. The UI never runs model HTTP or tools on the GPUI thread. The runtime never imports GPUI. Context collection runs on the main thread and must happen before `App::activate`, otherwise Crosspond is the frontmost app.

Computer tools target the **ambient** frontmost pid from when the launcher opened, not whichever app is frontmost after the hotkey. Presses and set-value run through Accessibility **without activating** the target (Codex-style background AX), so Crosspond stays frontmost and Chromium `AXPress`-at-origin does not hit traffic lights after a focus steal. Window chrome (close / minimize / zoom) is never pressed; text fields are focused instead of pressed.

Node ids are integers for the latest snapshot generation. A new snapshot or a successful UI action invalidates old ids.

Non-secret config is `~/.crosspond/config.json` (`provider`, `base_url`, `model`). The API key is only in Keychain (`com.crosspond.app` / `provider.api_key`). Config and key are loaded fresh on each StartTask and Test Connection.

A session reuses one workspace under `~/.crosspond/workspaces/<first-task-id>/`. Finder selections are copied into that workspace’s `input/` on the first turn. Each submit still writes `~/.crosspond/tasks/<task-id>/`. Closing the launcher sends `ResetSession`, which drops follow-up history, ambient context, and the session workspace.

The agent loop is capped at 16 steps. Tool output is capped at 100KB. Tools run on a blocking thread with a 30s timeout. Selected text is capped at 32,768 characters. AX snapshots cap depth, node count, and text length.

## Window show/hide

GPUI 0.2.2 has no per-window `hide()` / `show()`. The official `examples/window.rs` pattern is used:

- hide: `App::hide()` (`NSApplication hide:`)
- show: `App::activate(true)` plus `Window::activate_window()`

The launcher window is created once (`show: false`) and toggled; it is not destroyed on Escape.

`App::hide()` hides Settings as well. That is a known limitation of this GPUI version.

## Hotkeys

`GlobalHotkeyService` lives in `crosspond-core`. macOS registers Option + Space with `global-hotkey` on the main thread and exposes `poll()`. The GPUI app drains that poll on a short `Timer` loop. Settings-driven hotkeys come later; the trait is the extension point.

⌘, opens Settings. Escape cancels an in-flight request (including while waiting for approval); otherwise it hides the launcher. Approval **Cancel** rejects only that tool call.
