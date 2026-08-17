# Architecture

## Crates

| Crate | Role | May depend on |
| --- | --- | --- |
| `crosspond-app` | GPUI UI, process entry | core, macos, tools, GPUI |
| `crosspond-core` | runtime commands/events, agent loop, policy, receipts, context types | model, tools, knowledge, tokio, uuid |
| `crosspond-knowledge` | Obsidian-compatible Knowledge Vault (Markdown + YAML + derived SQLite FTS) | serde, uuid, rusqlite, notify; not GPUI, not core |
| `crosspond-model` | LLM provider abstraction | reqwest, serde |
| `crosspond-tools` | filesystem, computer, web, shell/URL, calendar, knowledge-lookup tool defs; backends as traits | serde, reqwest; not macos, not knowledge |
| `crosspond-macos` | hotkeys, Keychain, ambient context, cua-driver, EventKit | core, tools, platform crates, not GPUI |

`crosspond-model` must not depend on `crosspond-core`. `crosspond-knowledge` must not depend on GPUI or `crosspond-core`. `crosspond-tools` must not depend on `crosspond-macos` (core → tools and macos → core would cycle). macOS implements `AccessibilityBackend`, `ScreenshotBackend`, `AppBackend`, `InputBackend`, and `CalendarBackend` from tools.

The Knowledge Vault path is `config.json` `vault_path` (optional). It is not hard-coded under `~/.crosspond`. Markdown files are the source of truth; Crosspond creates `_system/Schema.md`, `Index.md`, and `Log.md` when opening a vault. Search state lives in `~/.crosspond/index/<vault-id>.sqlite` and can be rebuilt from the Markdown. When a vault is configured, `StartTask` runs `KnowledgeRouter` and injects a Knowledge Brief into the system prompt. Command prompts that match a Procedure also get a follow plan (requires before uses). The model reads notes through `knowledge_*` tools (`crosspond-tools` talks to a `KnowledgeBackend` trait; `crosspond-core` adapts `IndexedVault`). Tools must not depend on `crosspond-knowledge`. Computer use stays in the existing tool backends; Procedures are guidance, not a workflow DSL. Completed meaningful tasks write Activity notes under `history/YYYY/MM/` via `ActivityRecorder` (no raw traces). `knowledge_ingest` captures a Source and applies a validated `IngestionPlan` (provenance appends and links to retrieved candidates only; hash conflicts are reported, never overwritten). After a guided success with no existing Procedure, Crosspond asks to save a Procedure; the user must Allow, and the body is generated from the receipt rather than from unrestricted model writes. Read Later saves the current page, selection, PDF, or local document as an unread Source (`knowledge_read_later`); processing uses the same ingestion plan.

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
        │                      inject Knowledge Brief when vault_path is set
        │                      (procedure follow plan: requires → uses → computer tools)
        │                      OpenAI-compatible stream (text + images)
        │                      knowledge_search / knowledge_read /
        │                        knowledge_neighbors / knowledge_backlinks /
        │                        knowledge_find_procedure (auto; no note bodies in logs)
        │                      knowledge_ingest / knowledge_propose_update
        │                      knowledge_read_later / knowledge_archive_source
        │                        (validated plan; hash conflicts; no secrets)
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
        │                      write history/YYYY/MM/*.md Activity when a procedure
        │                        ran or the receipt has actions/artifacts
        │                      Save this as a Procedure? (Allow) after a guided
        │                        success with no existing Procedure
        │                      Read Later: unread Source from page / selection /
        │                        PDF / local doc (no selection in logs)
 Allow / Cancel  ──mpsc──►     Approve / Reject that action
  Escape / Stop   ──mpsc──►     Cancel the whole task
        │                      (Escape first closes History, then hides;
        │                       New sends ResetSession)
        │                      (hide keeps conversation and in-flight work)
 AgentEvent      ◄─mpsc──   ContextCollected / AssistantDelta /
        │                      ToolStarted / ApprovalRequired /
        │                      ArtifactCreated / TaskCompleted(+receipt) /
        │                      completed
        ▼
 Command window / Settings
 First launch (no API key): onboarding, then Settings. No Accessibility prompt.
 History reads ~/.crosspond/tasks/ (task.json + receipt.json).
```

Commands and events are defined in `crosspond-core`. The UI never runs model HTTP or tools on the GPUI thread. The runtime never imports GPUI. Context collection runs on the main thread and must happen before `App::activate`, otherwise Crosspond is the frontmost app. AX attribute reads use a short messaging timeout, Finder selection is killed after 800ms, and collect checks `AXIsProcessTrusted` without prompting (a TCC dialog while hidden looks like a freeze).

Computer tools default to the **ambient** frontmost pid from when the launcher opened. The model may pass `app` (display name or bundle id) on snapshot / screenshot / UI tools, or call `open_app` / `focus_app`, to drive another process. Snapshot, press, set-value, type, hotkey, scroll, screenshot, and click go through a host-spawned **cua-driver** MCP child (`mcp --direct` when the installed binary supports it, otherwise `mcp --no-daemon-relaunch`). Unrestricted computer-use is selected with `CUA_DRIVER_*` env vars; cua-driver 0.20+ rejects `--dangerously-bypass-approvals` on `mcp`. Crosspond keeps its own tool names and Allow cards; cua-driver’s full MCP catalog is not exposed to the model. Window chrome (close / minimize / zoom) is omitted from the snapshot; cua-driver delivers background actions so the user’s cursor is not moved.

`take_screenshot` asks cua-driver for that pid’s largest on-screen window image. `ui_click` sends exact pixels from that image (`delivery_mode: background`, `scope: window`); cua-driver owns Retina, window origin, and flipped-coordinate mapping. Element actions pass `element_token` and `snapshot_id` from the latest `get_window_state`. A successful click recaptures that same window (not the ambient frontmost app) and returns the fresh image. If the cua-driver child dies, cached screenshot coordinates are dropped so the next click cannot use a stale resize map. Only the newest screenshot image is kept in the model request; older images are dropped. Screenshot images are sent with vision `detail: high` and the PNG width×height in the accompanying text.

`ui_press` / `ui_set_value` address cua-driver `element_index` values with the matching `snapshot_id` (and `element_token` when present) from the latest snapshot. Prefer `ui_press` over `ui_click` whenever the control has a label in the tree.

`calendar_events` reads EventKit (not Calendar.app UI). Prefer it for schedule questions.

Node ids are integers for the latest Accessibility snapshot generation. A new snapshot or a successful UI action invalidates old ids.

Non-secret config is `~/.crosspond/config.json` (`provider`, `base_url`, `model`, `computer_approval`). API keys are only in Keychain (`com.crosspond.app` / `provider.api_key`, and optionally `exa.api_key`). Config and keys are loaded fresh on each StartTask and Test Connection. `computer_approval` is `manual` (ask every UI action), `auto` (run UI actions without asking), or `agent` (the model sets `ask_user` per call; omitted/`true` asks, `false` runs). External reads/writes, shell, and destructive tools still require Allow regardless of this setting. The launcher input row cycles the mode. **History** lists recent tasks. **New** sends `ResetSession`, which drops follow-up history, ambient context, and any session scratch handle. Hiding the launcher keeps any in-flight task running and preserves the conversation so the next show can continue chatting. Past receipts remain under `~/.crosspond/tasks/`. Legacy `~/.crosspond/workspaces/` directories are left untouched.

Tasks do not create a working directory on start. A scratch space under `~/.crosspond/scratch/<task-id>/` is created only when a file, download, or shell tool actually needs one (or when Finder selections are staged into `input/`). Follow-up turns in the same session reuse that scratch. Empty temporary scratches are removed when the task ends. Each submit still writes `~/.crosspond/tasks/<task-id>/` (`task.json`, `events.jsonl`, `receipt.json`).

The agent loop is capped at 16 steps. Tool output is capped at 100KB. Tools run on a blocking thread with a 30s timeout. Selected text is capped at 32,768 characters. AX snapshots cap depth, node count, and text length. Screenshot size is whatever cua-driver returns.

## Window show/hide

GPUI 0.2.2 has no per-window `hide()` / `show()`. The official `examples/window.rs` pattern is used:

- hide: `App::hide()` (`NSApplication hide:`)
- show: `App::activate(true)` plus `Window::activate_window()`

crates.io `gpui` 0.2.2 holds a `parking_lot` mutex across `resignKeyWindow` in `window_did_change_key_status`. AppKit delivers `windowDidResignKey` synchronously, re-enters the same function, and deadlocks the main thread (Not Responding). Crosspond uses `[patch.crates-io]` → `third_party/gpui`, which is that crates.io tree plus [zed#51035](https://github.com/zed-industries/zed/pull/51035) (drop the lock first). Do not replace this with Zed `main`.

The launcher window is created once (`show: false`) and toggled; it is not destroyed on Escape.

The compact idle command bar (no message sent yet, no History/onboarding overlay) hides when it loses key focus. An expanded conversation stays visible. Hide is skipped when Settings is also open, because `App::hide()` cannot hide only the launcher.

`App::hide()` hides Settings as well. That is a known limitation of this GPUI version.

First launch with no API key shows the launcher in onboarding and opens Settings from there. Accessibility is not requested until the user uses selected text or computer tools.

## Hotkeys

`GlobalHotkeyService` lives in `crosspond-core`. macOS registers Option + Space with `global-hotkey` on the main thread and exposes `poll()`. The GPUI app drains that poll on a short `Timer` loop, and handles the hotkey before applying queued agent events. If the launcher was ordered out (NSPanel `hidesOnDeactivate`) while the in-memory visible flag stayed true, Option+Space shows rather than calling `App::hide()`. Settings-driven hotkeys come later; the trait is the extension point.

⌘, opens Settings. ⌘N and ⌘T reset the session (same as **New**). ⌘W hides the launcher without cancelling work or clearing the conversation. Escape cancels an in-flight request (including while waiting for approval); closes History if it is open; otherwise it hides the launcher without clearing the conversation. **New** resets the session. Approval **Cancel** rejects only that tool call. Enter submits the prompt. Shift+Enter inserts a newline; the field grows with wrapped lines (capped) and pastes keep line breaks.
