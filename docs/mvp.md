# Crosspond MVP

> Command bar for your computer agent.

The user should never have to prepare the AI before asking for work. No projects, agent pickers, skill pickers, or manual workspace setup.

## Knowledge Vault Phase 8

Read Later is Source ingestion, not a separate silo. A current page URL, selected text, dropped PDF, or local document becomes an unread Source. Later, `knowledge_propose_update` runs the validated ingestion plan and marks it processed. Selected text and page URLs are not written to logs or receipts.

## Login fill

Resource notes may store a `credential_ref` Keychain pointer (never the secret). The pointer is bound to http(s) hosts on that note. Native login dialogs and Chromium HTTP basic/digest (browser chrome, not DOM) use `fill_credential`; the launcher collects username/password and shows the destination host or app. HTTP basic/digest **file servers** (listings and downloads) use `fetch_url`: unauthenticated HEAD first, then `fetch_url` again with `credential_ref` so the host can send Digest/Basic — not the browser, and not `curl --user`. Private LAN hosts work only when listed on the note. **Save in Keychain** is off by default and only overwrites a ref that already exists on a vault note. **Submit** on the login card is consent. Values never go to the model, logs, receipts, or the WebView.

## Knowledge Vault Phase 7

After `@vault-procedure` (on the command, or alone after a successful run), Crosspond asks **Save this as a Procedure?** on the existing Allow card. It does not auto-prompt after every guided success. Approving writes a Procedure from the sanitized receipt (not from arbitrary model Markdown) and links mentioned Resources. The next short command retrieves it.

## Knowledge Vault Phase 6

New Sources are fingerprinted, deduplicated, and turned into a validated `IngestionPlan` (candidates, creates, provenance appends, links, conflicts). Crosspond applies that plan; the model cannot patch arbitrary note bodies. Existing notes are never silently overwritten — hash conflicts stay in the plan. Secrets are refused.

## Knowledge Vault Phase 5

After a meaningful task (a matched Procedure, or computer/file work), Crosspond writes a readable Activity under `history/YYYY/MM/`. The note links the Procedure and Resources and stores the result, sanitized actions, and artifacts. It does not store tool JSON, model tokens, or chain-of-thought. A successful Procedure run may update `last_verified`. Simple Q&A does not create an Activity.

## Knowledge Vault Phase 4

When a command matches a Procedure, the brief includes a generic follow plan: required Resources first, then uses. The agent must `knowledge_read` those notes before computer tools and take app names, URLs, and paths from the Markdown — not from a hardcoded demo. Procedures still cannot bypass Allow.

## Knowledge Vault Phase 3

Before the model acts, Crosspond searches the vault and injects a short Knowledge Brief (titles, ids, snippets — not full notes). The agent can then call `knowledge_search`, `knowledge_read`, `knowledge_neighbors`, `knowledge_backlinks`, and `knowledge_find_procedure`. These lookups are read-only and do not need Allow. Vault Sources stay untrusted; Procedures cannot bypass Allow cards.

Acceptance: a vault with Check Lab Assignment / Lab VPN / Lab Wiki / Lab File Server, and the prompt `研究室の課題確認して`, lists Check Lab Assignment (and those resources) in the brief before execution.

## Knowledge Vault Phase 2

Markdown remains the source of truth. Crosspond keeps a disposable SQLite FTS5 cache outside the vault (`~/.crosspond/index/<vault-id>.sqlite`) for titles, aliases, tags, bodies, and the link graph. A filesystem watcher re-indexes Obsidian edits after a short debounce. Deleting the database and rebuilding from Markdown restores the same searchable state.

## Knowledge Vault Phase 1

Crosspond can open a user-chosen Obsidian-compatible vault and write normal Markdown notes with YAML frontmatter and `[[wikilinks]]`. Opening a vault creates `_system/Schema.md`, `Index.md`, and `Log.md` if they are missing. The vault path is `config.json` `vault_path`, set from Settings (default `~/Documents/Crosspond`), not a folder under `~/.crosspond`.

## Knowledge Vault Phase 0

Tasks no longer get a workspace directory on start. Chat and computer-use actions run without creating `~/.crosspond/scratch/<task-id>/`. A scratch space is created only when file processing, downloads, or shell execution need a working directory. Existing `~/.crosspond/workspaces/` data is left in place.

## Phase 12 (current product UI)

Polish: a human-readable receipt after each task, recent task history, and first-launch onboarding — on top of the Phase 11 Whole-Mac agent.

- **Receipt** — after a task, the launcher keeps the summary in the conversation and lists artifacts with **Show in Finder**. Receipts stay on disk as `receipt.json` (no secrets, calendar notes, command output, or typed text). Changed action lines are not shown in the launcher.
- **History** — **History** (or ↑ when the input is empty) lists recent conversations from `~/.crosspond/tasks/`, grouped by `conversation_id`. Opening an item shows the same transcript as the live chat (user turns, work steps, commentary, receipt) and the follow-up field continues that thread. **New** still starts a blank session.
- **Onboarding** — first launch with no provider ready (no Compatible API key, and not signed in with ChatGPT) shows a welcome and Settings. Do not prompt for Accessibility yet. After a key is saved or ChatGPT sign-in succeeds: “Press Option + Space anywhere” (or whatever shortcut is set in Settings). That shortcut and **Open** reveal the command bar; they do not dismiss the window. Settings is tabbed (General / Models / Knowledge / Search / Browser / Permissions). The Models tab can keep ChatGPT and several OpenAI Compatible endpoints at once; the launcher chooses the model (and Codex effort).
- **Mentions** — type `@` (or `＠`) in the compact bar to attach `@vault-query` (search accumulated knowledge), `@vault-save` (ingest), `@vault-later` (unread Source), `@vault-procedure` (save a Procedure from the last successful receipt), `@screen` (screenshot the ambient window), `@computer` (screenshot then operate the Mac with UI tools), `@browser` (operate the current Chromium tab with `browser_*` tools), `@app` (running apps from NSWorkspace), `@files`, `@calendar`, or `@search`. Mentions are optional; ambient context and the Knowledge Brief still run without them. The WebView only sees kinds and app names — not note bodies, paths, or screenshot bytes.

Demo: first launch → Settings → Option + Space → do work → see the transcript → hide → Option + Space → History → open a past conversation and follow up.

Phase 11 Whole-Mac tools still apply (any app, keyboard/scroll, shell/URL, EventKit, external file reads, web tools, earlier computer use).

## Phase 13 Browser

Chromium pages go through a Crosspond Chrome extension (CDP via `chrome.debugger`), not Accessibility or screenshots. The model calls `browser_snapshot` for a compact a11y outline with refs, then `browser_click` / `browser_fill` / `browser_type` on those refs. Native apps still use cua-driver AX / screenshot tools. HTTP basic/digest **in an open Chromium tab** is browser chrome, not DOM: the extension enables `Fetch.handleAuthRequests` without pausing every request (`patterns: []`), and the model calls `fill_credential` with only `credential_ref` for a host listed on that Resource. Cancel / reject / navigate aborts a paused challenge with `CancelAuth`. Lab file servers and other HTTP listings should use `fetch_url` (HEAD, then Digest/Basic from Keychain) so a browser's saved cookies cannot skip the login. Never `curl --user`.

The extension talks to Crosspond over native messaging (`com.crosspond.chrome`) and a user-only unix socket. Crosspond must be running first so it can copy `crosspond-chrome-host` to `~/.crosspond/bin` and write the native-host manifests. Load the extension unpacked from `extension/chrome` (Settings → Browser shows the path and connection badge). The extension reconnects if the native port drops and Crosspond retries in-flight `browser_*` calls; AX trees are compacted before they cross the 1 MiB Chrome host→extension cap. A new site host needs Allow in Manual and AI, then it is stored in `config.json` `browser_allowed_hosts`. Auto runs without asking and does not add the host. `browser_blocked_hosts` always refuses. Page bodies, cookies, and field values stay out of the WebView, receipts, and logs.

Safari, Firefox, Web Store listing, raw CDP, and an in-app browser are later.

## Phase 14 (distribution)

GitHub Releases ship an Apple Silicon `.dmg`. `/prepare-release` then `/release` drive Release Please: a version PR, user-facing `PUBLIC_CHANGELOG.md`, then a signed updater bundle (`latest.json` + `.app.tar.gz`). The launcher shows **Update available** on the header when a newer Release exists (dismissible for the process). Signing and notarization use the GitHub Environment `release` (Developer ID + updater minisign); only repository admins can run those workflows. The Chrome extension is still loaded unpacked from the app bundle path in Settings.

## Later phases (do not implement yet)

15. Chrome Web Store, Intel/universal Mac builds, lockfile license re-audit

## Non-goals for MVP

Projects, multi-agent orchestration, plugin/MCP marketplace, Crosspond-owned cloud accounts, Windows/Linux, local LLM runtime, drag, scheduled agents, voice, exposing cua-driver’s full MCP catalog, `kill_app`.

Personal ChatGPT Plus/Pro sign-in is an exception to “cloud accounts”: it uses the same public OAuth client as Codex CLI (not an official third-party subscription API). Crosspond does not create its own user accounts, and must not resell or multiplex one ChatGPT login across users.
