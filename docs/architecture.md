# Architecture

## Crates

| Crate | Role | May depend on |
| --- | --- | --- |
| `crosspond-app` | Tauri 2 host, process entry | core, macos, tools, Tauri |
| `ui/` | SvelteKit SPA (looks; invoke/events only) | `@tauri-apps/api`; not Rust crates |
| `crosspond-core` | runtime commands/events, agent loop, policy, receipts, context types | model, tools, knowledge, tokio, uuid |
| `crosspond-knowledge` | Obsidian-compatible Knowledge Vault (Markdown + YAML + derived SQLite FTS) | serde, uuid, rusqlite, notify; not Tauri, not core |
| `crosspond-model` | LLM provider abstraction | reqwest, serde |
| `crosspond-tools` | filesystem, computer, web, browser, shell/URL, calendar, knowledge-lookup tool defs; backends as traits | serde, reqwest; not macos, not knowledge |
| `crosspond-chrome-host` | native-messaging framing, unix-socket bridge, Chromium native-host manifests | serde_json; not Tauri, not core |
| `crosspond-macos` | hotkeys, Keychain, ambient context, cua-driver, EventKit | core, tools, platform crates, not Tauri |

`crosspond-model` must not depend on `crosspond-core`. `crosspond-knowledge` must not depend on Tauri or `crosspond-core`. `crosspond-tools` must not depend on `crosspond-macos` (core → tools and macos → core would cycle). macOS implements `AccessibilityBackend`, `ScreenshotBackend`, `AppBackend`, `InputBackend`, and `CalendarBackend` from tools. `crosspond-app` implements `BrowserBackend` by talking to the Chrome extension through `crosspond-chrome-host`. The MV3 extension lives in `extension/chrome/` and must not receive API keys.

The Knowledge Vault path is `config.json` `vault_path`, chosen in Settings (default `~/Documents/Crosspond`). It is not hard-coded under `~/.crosspond`. Markdown files are the source of truth; Crosspond creates `_system/Schema.md`, `Index.md`, and `Log.md` when opening a vault. Search state lives in `~/.crosspond/index/<vault-id>.sqlite` and can be rebuilt from the Markdown. When a vault is configured, `StartTask` runs `KnowledgeRouter` and injects a Knowledge Brief into the system prompt. Command prompts that match a Procedure also get a follow plan (requires before uses). The model reads notes through `knowledge_*` tools (`crosspond-tools` talks to a `KnowledgeBackend` trait; `crosspond-core` adapts `IndexedVault`). `knowledge_read` may expose a `credential_ref` pointer (never the secret); HTTP file servers use `fetch_url` with that pointer, and native/browser login uses `fill_credential`. Tools must not depend on `crosspond-knowledge`. Computer use stays in the existing tool backends; Procedures are guidance, not a workflow DSL. Completed meaningful tasks write Activity notes under `history/YYYY/MM/` via `ActivityRecorder` (no raw traces). `knowledge_ingest` captures a Source and applies a validated `IngestionPlan` (provenance appends and links to retrieved candidates only; hash conflicts are reported, never overwritten). After a guided success with no existing Procedure, Crosspond asks to save a Procedure; the user must Allow, and the body is generated from the receipt rather than from unrestricted model writes. Read Later saves the current page, selection, PDF, or local document as an unread Source (`knowledge_read_later`); processing uses the same ingestion plan.

## Agent data flow

```
Tauri main thread + WebView              Tokio runtime thread
────────────────────────────             ────────────────────
Option+Space
        │
 collect context  (before show/focus)
        │
    show launcher + badge_lines
        │
 Enter / start_task(+capsule, mentions) ──mpsc──►  stage Finder files into scratch input/ only if selected
        │                      inject ambient block into system prompt
        │                      honor @vault-query/@vault-save/@vault-later/@screen/@computer/@browser/@app/@files/@calendar/@web
        │                      @screen and @computer capture ambient pid before the model runs
        │                      @computer also requires snapshot / UI tools, not look-only
        │                      @browser requires browser_snapshot / browser_* refs, not AX or screenshots
        │                      @vault-query tells the model to knowledge_search then knowledge_read
        │                      inject Knowledge Brief when vault_path is set
        │                      (procedure follow plan: requires → uses → computer tools)
        │                      OpenAI-compatible stream, or ChatGPT Codex Responses
        │                      (text + images; OAuth tokens never leave Rust)
        │                      knowledge_search / knowledge_read /
        │                        knowledge_neighbors / knowledge_backlinks /
        │                        knowledge_find_procedure (auto; no note bodies in logs)
        │                      knowledge_ingest / knowledge_propose_update
        │                      knowledge_read_later / knowledge_archive_source
        │                        (validated plan; hash conflicts; no secrets)
        │                      fs tools (scratch auto when needed; external after Allow,
        │                        or Auto)
        │                      list_apps / open_app / focus_app
        │                      get_accessibility_snapshot / take_screenshot
        │                        (optional app= retarget; else ambient pid)
        │                      browser_tabs / browser_snapshot / browser_text
        │                        (Chrome extension + CDP; not AX)
        │                      browser_click / browser_fill / browser_type /
        │                        browser_press_key / browser_scroll /
        │                        browser_select / browser_navigate /
        │                        browser_new_tab
        │                        unknown site host → Allow in Manual/AI (Auto skips
        │                          and does not persist the host)
        │                        then ComputerAction policy for writes
        │                      web_search / fetch_url (auto; Exa key for search)
        │                      calendar_events (auto; EventKit / Calendar TCC)
        │                      ui_type / ui_hotkey / ui_scroll
        │                      ui_press / ui_set_value / ui_click
        │                      fill_credential (Keychain miss → CredentialRequired;
        │                        SubmitCredential; values never in AgentEvent)
        │                        Manual → ApprovalRequired
        │                        Agent + ask_user → ApprovalRequired
        │                        Auto (and Agent + ask_user false) skip the card
        │                      run_command / open_url (non-http)
        │                        Manual / Agent → Allow card
        │                        Auto skip the card
        │                      write ~/.crosspond/tasks/<task-id>/
        │                        (task.json with conversation_id,
        │                         UI-safe events.jsonl, sanitized session.json,
        │                         receipt.json)
        │                      write history/YYYY/MM/*.md Activity when a procedure
        │                        ran or the receipt has actions/artifacts
        │                      Save this as a Procedure? (Allow) after a guided
        │                        success with no existing Procedure
        │                      Read Later: unread Source from page / selection /
        │                        PDF / local doc (no selection in logs)
 Allow / Cancel  ──mpsc──►     Approve / Reject that action
 Fill login      ──mpsc──►     SubmitCredential (values stay in Rust)
  Escape / Stop   ──mpsc──►     Cancel the whole task
        │                      (Escape first closes History, then hides;
        │                       New sends ResetSession)
        │                      (hide keeps conversation and in-flight work)
 AgentEvent      ◄─mpsc──   ContextCollected / AssistantDelta /
        │                      ToolStarted / ApprovalRequired /
        │                      CredentialRequired /
        │                      ArtifactCreated / TaskCompleted(+receipt) /
        │                      completed
        ▼
 Command window / Settings
 First launch (no API key / ChatGPT session): onboarding, then Settings. No Accessibility prompt.
 History reads ~/.crosspond/tasks/ grouped by conversation_id
 (task.json + events.jsonl + session.json + receipt.json).
 Opening a conversation hydrates the transcript and ResumeSession.
```

Commands and events are defined in `crosspond-core`. The UI never runs model HTTP or tools on the Tauri/WebView thread. The runtime never imports Tauri or Svelte. Context collection runs on the main thread and must happen before the launcher is shown and focused, otherwise Crosspond is the frontmost app. The Tauri host sets `NSApplicationActivationPolicyAccessory` at launch (`LSUIElement` in Info.plist; tao otherwise forces Regular). Snapshot/screenshot targeting skips Crosspond and falls back to the menu-bar owner or the frontmost on-screen window of another app. AX attribute reads use a short messaging timeout, Finder selection is killed after 800ms, and collect checks `AXIsProcessTrusted` without prompting (a TCC dialog while hidden looks like a freeze). The WebView receives `AgentEvent` JSON and `badge_lines` only — not selected text, Finder paths, secrets, or login values. Mid-turn `AssistantDelta` stays in the transcript as user-visible commentary; thinking and tool rows collapse into a work header. That commentary must not include selected text, passwords, calendar notes, or field values.

Computer tools default to the **ambient** frontmost pid from when the launcher opened. The model may pass `app` (display name or bundle id) on snapshot / screenshot / UI tools, or call `open_app` / `focus_app`, to drive another process. Snapshot, press, set-value, type, hotkey, scroll, screenshot, and click go through a host-spawned **cua-driver** MCP child (`mcp --direct` when the installed binary supports it, otherwise `mcp --no-daemon-relaunch`). Unrestricted computer-use is selected with `CUA_DRIVER_*` env vars; cua-driver 0.20+ rejects `--dangerously-bypass-approvals` on `mcp`. Crosspond keeps its own tool names and Allow cards; cua-driver’s full MCP catalog is not exposed to the model. Window chrome (close / minimize / zoom) is omitted from the snapshot; cua-driver delivers background actions so the user’s cursor is not moved.

`take_screenshot` asks cua-driver for that pid’s largest on-screen window image. `ui_click` sends exact pixels from that image (`delivery_mode: background`, `scope: window`); cua-driver owns Retina, window origin, and flipped-coordinate mapping. Element actions pass `element_token` and `snapshot_id` from the latest `get_window_state`. A successful click recaptures that same window (not the ambient frontmost app) and returns the fresh image. If the cua-driver child dies, or it reports that the computer-use session has ended, cached screenshot coordinates are dropped so the next click cannot use a stale resize map. A session-ended error also kills that MCP child and retries the call once on a fresh connection. Only the newest screenshot image is kept in the model request; older images are dropped. Screenshot images are sent with vision `detail: high` and the PNG width×height in the accompanying text.

`ui_press` / `ui_set_value` address cua-driver `element_index` values with the matching `snapshot_id` (and `element_token` when present) from the latest snapshot. Prefer `ui_press` over `ui_click` whenever the control has a label in the tree. Password fields refuse `ui_set_value` and `ui_type`; the host fills them with `fill_credential` from a vault `credential_ref` (user or Keychain), never from model-supplied values. HTTP basic/digest file servers use `fetch_url` with the same `credential_ref` (HEAD, then host Digest/Basic). Chromium HTTP basic/digest in an open tab uses `fill_credential` without Accessibility node ids after `browser_navigate` reports the challenge.

Chromium tabs with the Crosspond extension connected use `browser_*` tools instead of AX/screenshots. Snapshot refs (`{epoch}-eN`) are valid only until the next snapshot or navigation. Password, OTP, email, and phone field values are `••••`. HTTP basic/digest is intercepted with CDP `Fetch.authRequired`; continue with `fill_credential` (Keychain / login card), not `browser_fill` or curl. File-server listings should prefer `fetch_url` so a browser profile's cookies cannot skip auth. If the extension is disconnected, those tools say how to load it and must not silently fall back to Accessibility. New website hosts need Allow in Manual and AI, then join `browser_allowed_hosts` in `config.json`. Auto runs browser tools on unknown hosts without asking and does not add them to Allowed Sites. Blocked hosts are refused. The native host is `com.crosspond.chrome`. On launch, Crosspond copies `crosspond-chrome-host` to `~/.crosspond/bin` and writes Chromium native-host manifests; Chrome then launches that binary, which forwards length-prefixed JSON to `~/.crosspond/chrome-bridge.sock`. The extension strips AX trees before posting them over native messaging so a large page cannot drop the port. If the port still drops, the service worker reconnects (alarms + tab focus) and Crosspond retries the call for a few seconds. The host process exits when Chrome closes stdio so the next `connectNative` can launch a clean one.

`calendar_events` reads EventKit (not Calendar.app UI). Prefer it for schedule questions.

Node ids are integers for the latest Accessibility snapshot generation. A new snapshot or a successful UI action invalidates old ids.

Non-secret config is `~/.crosspond/config.json` (`openai_compat` endpoints with `id` / `name` / `base_url`, `selected` `{ source, model }`, `reasoning_effort`, `computer_approval`, `vault_path`, `launcher_hotkey`, `browser_allowed_hosts`, `browser_blocked_hosts`). `selected.source` is `chatgpt` or a Compatible endpoint id. ChatGPT is available when Keychain has an OAuth blob; Compatible endpoints are listed in config and may all have keys at once. Old `provider` / `base_url` / `model` files migrate to one Compatible endpoint (`id: default`) and, if `provider` was `chatgpt_codex`, `selected` ChatGPT. API keys and ChatGPT OAuth tokens are only in Keychain (`com.crosspond.app` / `provider.api_key` for the default Compatible endpoint, `provider.api_key.{id}` for extras, optional `exa.api_key`, and `provider.chatgpt_oauth` as one JSON blob). Vault logins use `credential.{ref}` (one JSON username/password bundle per existing note pointer). Config and secrets are loaded fresh on each StartTask and Test Connection. The selected model and effort are chosen in the launcher; Settings only stores connection info. Model ids for the launcher dropdown are fetched in Rust (`GET {base}/models` or Codex `GET https://chatgpt.com/backend-api/codex/models`) and the WebView receives ids/labels only — never API keys, JWTs, account ids, or login values. `reasoning.effort` is sent on Codex Responses only, never on OpenAI Compatible Chat Completions. The WebView may see `chatgpt_signed_in: bool` and a short status string — never access tokens, refresh tokens, JWTs, or account ids. Encrypted Codex reasoning (`encrypted_content`) stays on in-memory `Message` objects for the live session; it is omitted from `events.jsonl`, `session.json`, receipts, `Debug`, and the WebView. History / ResumeSession restore that sanitized `session.json`, so a ChatGPT thread opened after restart cannot send prior `encrypted_content` (live follow-ups in the same process still can). `launcher_hotkey` is a shortcut spec such as `alt+Space` (default, Option+Space). Changing it in Settings re-registers the global hotkey immediately. `computer_approval` is `manual` (ask before UI actions, shell, external files, and non-http URLs), `auto` (run every tool without asking), or `agent` (the model sets `ask_user` per computer-action call; omitted/`true` asks, `false` runs; shell, external files, and non-http URLs still require Allow). The launcher input row cycles the mode. **History** lists recent conversations. Opening one restores the transcript and a sanitized model history so a follow-up continues that thread. **New** sends `ResetSession`, which drops follow-up history, ambient context, and any session scratch handle. Hiding the launcher keeps any in-flight task running and preserves the conversation so the next show can continue chatting. Past conversations remain under `~/.crosspond/tasks/`. Legacy `~/.crosspond/workspaces/` directories are left untouched.

Tasks do not create a working directory on start. A scratch space under `~/.crosspond/scratch/<task-id>/` is created only when a file, download, or shell tool actually needs one (or when Finder selections are staged into `input/`). Follow-up turns in the same session reuse that scratch. Empty temporary scratches are removed when the task ends. Each submit still writes `~/.crosspond/tasks/<task-id>/` (`task.json` with `conversation_id`, UI-safe `events.jsonl`, sanitized `session.json`, `receipt.json`). Follow-up turns in the same conversation share that id. ResumeSession loads the latest sanitized session for the conversation; tool bodies, screenshot bytes, and raw tool arguments are not restored.

The agent loop runs until the model returns a final answer or the user cancels (Escape / Stop). Tool output is capped at 100KB. Tools run on a blocking thread with a 30s timeout. Selected text is capped at 32,768 characters. AX snapshots cap depth, node count, and text length. Screenshot size is whatever cua-driver returns.

## Window show/hide

The launcher and Settings are separate Tauri windows. Hide/show is per-window (`WebviewWindow::hide` / `show`). Settings stays up when the launcher hides.

The launcher window is created once (`visible: false`) and toggled; it is not destroyed on Escape. It is frameless and transparent. Compact idle height is about 112px plus badge lines (tab header, input, and a slim model / effort / Auto footer). Opening a conversation resizes to about 560px. A compact bar that grew for badges or wrapped input is still compact — the first message (including after **New**) must expand. Streaming progress and a user resize of an already-expanded window must not snap it back to 560px. Compact-then-expand requests are sequenced so New cannot shrink the window after send has already asked for 560px.

The compact idle command bar (no message sent yet, no History/onboarding overlay) hides when Crosspond is no longer the active app. An expanded conversation stays visible. Hide is skipped when Settings is also open. Hide is also skipped while Japanese IME (or another in-app palette) has key without deactivating the app — WKWebView owns IME, and IME candidate windows typically keep the app active. The WebView also reports the `@` mention picker and a pending Allow or login card as composing so click-away does not hide mid-pick. Model, effort, and computer-approval use native `<select>` so WKWebView shows the macOS popup above the launcher (no in-window menu, no compact resize). Focus on those controls is still reported as composing so hide-on-blur does not fire while the system menu is open. The compact window grows for mention chips and the `@` list; `@` / `＠` stay in a textarea (not contenteditable) so WKWebView keeps IME.

First launch with no provider ready (no Compatible API key, and not signed in with ChatGPT) shows the launcher in onboarding and opens Settings from there. After a key is saved or ChatGPT sign-in succeeds, **Open** (or the launcher hotkey) reveals the command bar in the same window — it does not hide. Accessibility is not requested until the user uses selected text or computer tools.

## ChatGPT Codex OAuth

`chatgpt_codex` is an optional ChatGPT connection alongside any number of OpenAI Compatible endpoints. It reuses the public OAuth client shipped with Codex CLI (`app_EMoamEEZ73f0CkXaXp7hrann`) and talks only to `https://chatgpt.com/backend-api/codex/responses` (and `GET .../codex/models?client_version=0.144.1` for Test Connection and the launcher list). Requests send `originator: codex_cli_rs` and `version: 0.144.1`. The models GET also requires the `client_version` query; the `version` header alone is not enough. Tokens must not be sent to `api.openai.com`. This is not an official third-party ChatGPT API; Crosspond uses it for personal Plus/Pro sessions only and must not resell or multiplex one login. Crosspond keeps its own system prompt and tool names (no OpenCode Codex-bridge prompt). Default ChatGPT model is `gpt-5.6-luna`. Codex Responses may include `reasoning.effort` (`none` / `low` / `medium` / `high` / `xhigh`); Compatible Chat Completions never get that field.

Login lives in `crosspond-app`: PKCE, then `tauri-plugin-opener` to the authorize URL, then a localhost listener on `127.0.0.1:1455`. The listener ignores non-callback requests (favicon, empty connections) until `/auth/callback` arrives with a matching `state`, or the user cancels. A second Sign in cancels the previous wait. If that port is already bound (often Codex CLI), Settings shows the URL and the user pastes the full redirect (including `state`). Sign-out deletes the Keychain blob and, if ChatGPT was the selected launcher model, switches selected to the first Compatible endpoint. Token refresh is serialized process-wide in `crosspond-model` via `ChatGptTokenStore` (core backs it with Keychain) so Test Connection cannot rotate away a live chat’s refresh token. Refresh responses may omit `refresh_token`; Crosspond keeps the previous one.

## Hotkeys

`GlobalHotkeyService` lives in `crosspond-core`. macOS registers the configured launcher shortcut (default Option + Space) with `global-hotkey` on the main thread and exposes `poll()`. The Tauri host drains that poll on a short loop off the UI thread, then toggles the launcher on the main thread (collect, then show). If the launcher was ordered out while the in-memory visible flag stayed true, the hotkey shows rather than hiding. If first-launch onboarding is visible and a provider is ready (ChatGPT signed in or a Compatible key is stored), the hotkey reveals the command bar instead of hiding. Settings records a new shortcut (`set_launcher_hotkey`) and re-registers it on the main thread; `LauncherHotkey` in config is the portable spec. While the recorder is open, the host unregisters the current shortcut (`pause_launcher_hotkey`) and ignores queued toggles so that combination reaches Settings instead of showing the launcher. Escape, a failed capture, or closing Settings restores it (`resume_launcher_hotkey`).

⌘, opens Settings. ⌘N and ⌘T reset the session (same as **New**). ⌘W hides the launcher without cancelling work or clearing the conversation. Escape closes the mention picker first; native model / effort / approval menus handle Escape themselves. Then it cancels an in-flight request (including while waiting for approval); closes History if it is open; otherwise it hides the launcher without clearing the conversation. **New** resets the session. Approval **Cancel** rejects only that tool call. Enter submits the prompt (or selects the highlighted mention). Shift+Enter inserts a newline; the field grows with wrapped lines (capped) and pastes keep line breaks. Type `@` or `＠` after whitespace to attach optional mentions (`@vault-query`, `@vault-save`, `@vault-later`, `@screen`, `@computer`, `@browser`, `@app`, `@files`, `@calendar`, `@web`). `@vault-query` searches accumulated knowledge; the user does not pick a note. `@app` lists running apps via NSWorkspace (not cua-driver). `@screen` screenshots the ambient window from launcher-open time, not Crosspond. `@computer` attaches that screenshot and requires operating the Mac with snapshot / UI tools. `@browser` tells the model to operate the current Chromium tab with `browser_snapshot` and `browser_*` refs (not Accessibility or screenshots). ⌘C / ⌘V / ⌘X / ⌘A / ⌘Z are native Edit menu items so WKWebView can copy, paste, and undo (the app menu is otherwise only Settings and Quit).
