# Architecture

## Crates

| Crate | Role | May depend on |
| --- | --- | --- |
| `crosspond-app` | GPUI UI, process entry | core, macos, tools, GPUI |
| `crosspond-core` | runtime commands/events, agent loop, policy, receipts, context types | model, tools, knowledge, tokio, uuid |
| `crosspond-knowledge` | Obsidian-compatible Knowledge Vault (Markdown + YAML + derived SQLite FTS) | serde, uuid, rusqlite, notify; not GPUI, not core |
| `crosspond-model` | LLM provider abstraction | reqwest, serde |
| `crosspond-tools` | filesystem, computer, web, shell/URL, calendar tool defs; backends as traits | serde, reqwest; not macos |
| `crosspond-macos` | hotkeys, Keychain, ambient context, cua-driver, EventKit | core, tools, platform crates, not GPUI |

`crosspond-model` must not depend on `crosspond-core`. `crosspond-knowledge` must not depend on GPUI or `crosspond-core`. `crosspond-tools` must not depend on `crosspond-macos` (core → tools and macos → core would cycle). macOS implements `AccessibilityBackend`, `ScreenshotBackend`, `AppBackend`, `InputBackend`, and `CalendarBackend` from tools.

The Knowledge Vault path is `config.json` `vault_path` (optional). It is not hard-coded under `~/.crosspond`. Markdown files are the source of truth; Crosspond creates `_system/Schema.md`, `Index.md`, and `Log.md` when opening a vault. Search state lives in `~/.crosspond/index/<vault-id>.sqlite` and can be rebuilt from the Markdown.

## Agent data flow

```
GPUI thread                          Tokio runtime thread
───────────                          ────────────────────
Option+Space
        │
 collect context  (before activate)
        │
    show launcher + badges
        │
 Enter / StartTask(+capsule) ──mpsc──►  stage Finder files into scratch input/ only if selected
        │                      inject ambient block into system prompt
        │                      OpenAI-compatible stream (text + images)
        │                      fs tools (scratch auto when needed; external after Allow)
        │                      list_apps / open_app / focus_app
        │                      get_accessibility_snapshot / take_screenshot
        │                        (optional app= retarget; else ambient pid)
        │                      web_search / fetch_url (auto; Exa key for search)
        │                      calendar_events (auto; EventKit / Calendar TCC)
        │                      ui_type / ui_hotkey / ui_scroll
        │                      ui_press / ui_set_value / ui_click
        │                        Manual → ApprovalRequired
        │                        Agent + ask_user → ApprovalRequired
        │                        Auto (and Agent + ask_user false) skip the card
        │                      run_command / open_url (non-http) → Allow
        │                      write receipt.json under ~/.crosspond/tasks/<task-id>/
 Allow / Cancel  ──mpsc──►     Approve / Reject that action
 Escape / Stop   ──mpsc──►     Cancel the whole task
        │                      (Escape first closes History, then hides)
 AgentEvent      ◄─mpsc──   ContextCollected / AssistantDelta /
        │                      ToolStarted / ApprovalRequired /
        │                      ArtifactCreated / TaskCompleted(+receipt) /
        │                      completed
        ▼
 Command window / Settings
 First launch (no API key): onboarding, then Settings. No Accessibility prompt.
 History reads ~/.crosspond/tasks/ (task.json + receipt.json).
```

Commands and events are defined in `crosspond-core`. The UI never runs model HTTP or tools on the GPUI thread. The runtime never imports GPUI. Context collection runs on the main thread and must happen before `App::activate`, otherwise Crosspond is the frontmost app.

Computer tools default to the **ambient** frontmost pid from when the launcher opened. The model may pass `app` (display name or bundle id) on snapshot / screenshot / UI tools, or call `open_app` / `focus_app`, to drive another process. Snapshot, press, set-value, type, hotkey, scroll, screenshot, and click go through a host-spawned **cua-driver** MCP child (`mcp --direct` when the installed binary supports it, otherwise `mcp --no-daemon-relaunch`). Crosspond keeps its own tool names and Allow cards; cua-driver’s full MCP catalog is not exposed to the model. Window chrome (close / minimize / zoom) is omitted from the snapshot; cua-driver delivers background actions so the user’s cursor is not moved.

`take_screenshot` asks cua-driver for that pid’s largest on-screen window image. `ui_click` sends exact pixels from that image (`delivery_mode: background`); cua-driver owns Retina, window origin, and flipped-coordinate mapping. A successful click invalidates its input image and returns a fresh post-click screenshot for model verification. Only the newest screenshot image is kept in the model request; older images are dropped.

`ui_press` / `ui_set_value` address cua-driver `element_index` values (and `element_token` when present) from the latest snapshot. Prefer `ui_press` over `ui_click` whenever the control has a label in the tree.

`calendar_events` reads EventKit (not Calendar.app UI). Prefer it for schedule questions.

Node ids are integers for the latest Accessibility snapshot generation. A new snapshot or a successful UI action invalidates old ids.

Non-secret config is `~/.crosspond/config.json` (`provider`, `base_url`, `model`, `computer_approval`). API keys are only in Keychain (`com.crosspond.app` / `provider.api_key`, and optionally `exa.api_key`). Config and keys are loaded fresh on each StartTask and Test Connection. `computer_approval` is `manual` (ask every UI action), `auto` (run UI actions without asking), or `agent` (the model sets `ask_user` per call; omitted/`true` asks, `false` runs). External reads/writes, shell, and destructive tools still require Allow regardless of this setting. The launcher input row cycles the mode. **History** lists recent tasks. Closing the launcher sends `ResetSession`, which drops follow-up history, ambient context, and any session scratch handle. Past receipts remain under `~/.crosspond/tasks/`. Legacy `~/.crosspond/workspaces/` directories are left untouched.

Tasks do not create a working directory on start. A scratch space under `~/.crosspond/scratch/<task-id>/` is created only when a file, download, or shell tool actually needs one (or when Finder selections are staged into `input/`). Follow-up turns in the same session reuse that scratch. Empty temporary scratches are removed when the task ends. Each submit still writes `~/.crosspond/tasks/<task-id>/` (`task.json`, `events.jsonl`, `receipt.json`).

The agent loop is capped at 16 steps. Tool output is capped at 100KB. Tools run on a blocking thread with a 30s timeout. Selected text is capped at 32,768 characters. AX snapshots cap depth, node count, and text length. Screenshot size is whatever cua-driver returns.

## Window show/hide

GPUI 0.2.2 has no per-window `hide()` / `show()`. The official `examples/window.rs` pattern is used:

- hide: `App::hide()` (`NSApplication hide:`)
- show: `App::activate(true)` plus `Window::activate_window()`

The launcher window is created once (`show: false`) and toggled; it is not destroyed on Escape.

`App::hide()` hides Settings as well. That is a known limitation of this GPUI version.

First launch with no API key shows the launcher in onboarding and opens Settings from there. Accessibility is not requested until the user uses selected text or computer tools.

## Hotkeys

`GlobalHotkeyService` lives in `crosspond-core`. macOS registers Option + Space with `global-hotkey` on the main thread and exposes `poll()`. The GPUI app drains that poll on a short `Timer` loop. Settings-driven hotkeys come later; the trait is the extension point.

⌘, opens Settings. Escape cancels an in-flight request (including while waiting for approval); closes History if it is open; otherwise it hides the launcher. Approval **Cancel** rejects only that tool call.
