# Crosspond MVP

> Command bar for your computer agent.

The user should never have to prepare the AI before asking for work. No projects, agent pickers, skill pickers, or manual workspace setup.

## Knowledge Vault Phase 4

When a command matches a Procedure, the brief includes a generic follow plan: required Resources first, then uses. The agent must `knowledge_read` those notes before computer tools and take app names, URLs, and paths from the Markdown — not from a hardcoded demo. Procedures still cannot bypass Allow.

## Knowledge Vault Phase 3

Before the model acts, Crosspond searches the vault and injects a short Knowledge Brief (titles, ids, snippets — not full notes). The agent can then call `knowledge_search`, `knowledge_read`, `knowledge_neighbors`, `knowledge_backlinks`, and `knowledge_find_procedure`. These lookups are read-only and do not need Allow. Vault Sources stay untrusted; Procedures cannot bypass Allow cards.

Acceptance: a vault with Check Lab Assignment / Lab VPN / Lab Wiki / Lab File Server, and the prompt `研究室の課題確認して`, lists Check Lab Assignment (and those resources) in the brief before execution.

## Knowledge Vault Phase 2

Markdown remains the source of truth. Crosspond keeps a disposable SQLite FTS5 cache outside the vault (`~/.crosspond/index/<vault-id>.sqlite`) for titles, aliases, tags, bodies, and the link graph. A filesystem watcher re-indexes Obsidian edits after a short debounce. Deleting the database and rebuilding from Markdown restores the same searchable state.

## Knowledge Vault Phase 1

Crosspond can open a user-chosen Obsidian-compatible vault and write normal Markdown notes with YAML frontmatter and `[[wikilinks]]`. Opening a vault creates `_system/Schema.md`, `Index.md`, and `Log.md` if they are missing. The vault path is `config.json` `vault_path`, not a folder under `~/.crosspond`.

## Knowledge Vault Phase 0

Tasks no longer get a workspace directory on start. Chat and computer-use actions run without creating `~/.crosspond/scratch/<task-id>/`. A scratch space is created only when file processing, downloads, or shell execution need a working directory. Existing `~/.crosspond/workspaces/` data is left in place.

## Phase 12 (current product UI)

Polish: a human-readable receipt after each task, recent task history, and first-launch onboarding — on top of the Phase 11 Whole-Mac agent.

- **Receipt** — after a task, the launcher shows Done, changed actions, and artifacts with **Show in Finder**. Receipts stay on disk as `receipt.json` (no secrets, calendar notes, command output, or typed text).
- **History** — **History** (or ↑ when the input is empty) lists recent tasks from `~/.crosspond/tasks/`. This is task history, not a chat sidebar. Opening an item shows that receipt; it does not resume the conversation.
- **Onboarding** — first launch with no API key shows a welcome and Settings. Do not prompt for Accessibility yet. After a key is saved: “Press Option + Space anywhere.”
- **Settings → Permissions** — Accessibility, Screen Recording, and Calendars with **Open System Settings**. Chat still works if they are off.

Demo: first launch → Settings → Option + Space → do work → see the receipt → hide → Option + Space → History → open a past task.

Phase 11 Whole-Mac tools still apply (any app, keyboard/scroll, shell/URL, EventKit, external file reads, web tools, earlier computer use).

## Later phases (do not implement yet)

13. Release (signing, notarization, license re-audit)

## Non-goals for MVP

Projects, multi-agent orchestration, plugin/MCP marketplace, cloud accounts, Windows/Linux, local LLM runtime, drag, scheduled agents, voice, exposing cua-driver’s full MCP catalog, `kill_app`.
