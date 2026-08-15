# Architecture

## Crates

| Crate | Role | May depend on |
| --- | --- | --- |
| `crosspond-app` | GPUI UI, process entry | core, macos, tools, GPUI |
| `crosspond-core` | runtime commands/events, agent loop, policy, receipts, context types | model, tools, tokio, uuid |
| `crosspond-model` | LLM provider abstraction | reqwest, serde |
| `crosspond-tools` | filesystem tools, computer-tool defs, `AccessibilityBackend`, `ScreenshotBackend` | serde; not macos |
| `crosspond-macos` | hotkeys, Keychain, ambient context, cua-driver computer use | core, tools, platform crates, not GPUI |

`crosspond-model` must not depend on `crosspond-core`. `crosspond-tools` must not depend on `crosspond-macos` (core → tools and macos → core would cycle). macOS implements `AccessibilityBackend` and `ScreenshotBackend` from tools.

## Phase 5 data flow

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
        │                      OpenAI-compatible stream (text + images)
        │                      fs tools (workspace auto; external after Allow)
        │                      get_accessibility_snapshot (auto)
        │                      take_screenshot (auto) → tool text + image
        │                      ui_press / ui_set_value / ui_click
        │                        Manual → ApprovalRequired
        │                        Agent + ask_user → ApprovalRequired
        │                        Auto (and Agent + ask_user false) skip the card
 Allow / Cancel  ──mpsc──►     Approve / Reject that action
 Escape / Stop   ──mpsc──►     Cancel the whole task
 AgentEvent      ◄─mpsc──   ContextCollected / AssistantDelta /
        │                      ToolStarted / ApprovalRequired /
        │                      ArtifactCreated / completed
        ▼
 Command window / Settings
```

Commands and events are defined in `crosspond-core`. The UI never runs model HTTP or tools on the GPUI thread. The runtime never imports GPUI. Context collection runs on the main thread and must happen before `App::activate`, otherwise Crosspond is the frontmost app.

Computer tools target the **ambient** frontmost pid from when the launcher opened, not whichever app is frontmost after the hotkey. Snapshot, press, set-value, screenshot, and click all go through a host-spawned **cua-driver** MCP child (`mcp --direct` when the installed binary supports it, otherwise `mcp --no-daemon-relaunch`). Crosspond keeps its own tool names and Allow cards; cua-driver’s full MCP catalog is not exposed to the model. Window chrome (close / minimize / zoom) is omitted from the snapshot; cua-driver delivers background actions so the user’s cursor is not moved.

`take_screenshot` asks cua-driver for that pid’s largest on-screen window image. `ui_click` sends exact pixels from that image (`delivery_mode: background`); cua-driver owns Retina, window origin, and flipped-coordinate mapping. A successful click invalidates its input image and returns a fresh post-click screenshot for model verification. Only the newest screenshot image is kept in the model request; older images are dropped.

`ui_press` / `ui_set_value` address cua-driver `element_index` values (and `element_token` when present) from the latest snapshot. Prefer `ui_press` over `ui_click` whenever the control has a label in the tree.

Node ids are integers for the latest Accessibility snapshot generation. A new snapshot or a successful UI action invalidates old ids.

Non-secret config is `~/.crosspond/config.json` (`provider`, `base_url`, `model`, `computer_approval`). The API key is only in Keychain (`com.crosspond.app` / `provider.api_key`). Config and key are loaded fresh on each StartTask and Test Connection. `computer_approval` is `manual` (ask every UI action), `auto` (run UI actions without asking), or `agent` (the model sets `ask_user` per call; omitted/`true` asks, `false` runs). External writes, shell, and destructive tools still require Allow regardless of this setting. The launcher input row cycles the mode.

A session reuses one workspace under `~/.crosspond/workspaces/<first-task-id>/`. Finder selections are copied into that workspace’s `input/` on the first turn. Each submit still writes `~/.crosspond/tasks/<task-id>/`. Closing the launcher sends `ResetSession`, which drops follow-up history, ambient context, and the session workspace.

The agent loop is capped at 16 steps. Tool output is capped at 100KB. Tools run on a blocking thread with a 30s timeout. Selected text is capped at 32,768 characters. AX snapshots cap depth, node count, and text length. Screenshot size is whatever cua-driver returns.

## Window show/hide

GPUI 0.2.2 has no per-window `hide()` / `show()`. The official `examples/window.rs` pattern is used:

- hide: `App::hide()` (`NSApplication hide:`)
- show: `App::activate(true)` plus `Window::activate_window()`

The launcher window is created once (`show: false`) and toggled; it is not destroyed on Escape.

`App::hide()` hides Settings as well. That is a known limitation of this GPUI version.

## Hotkeys

`GlobalHotkeyService` lives in `crosspond-core`. macOS registers Option + Space with `global-hotkey` on the main thread and exposes `poll()`. The GPUI app drains that poll on a short `Timer` loop. Settings-driven hotkeys come later; the trait is the extension point.

⌘, opens Settings. Escape cancels an in-flight request (including while waiting for approval); otherwise it hides the launcher. Approval **Cancel** rejects only that tool call.
