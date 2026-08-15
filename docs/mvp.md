# Crosspond MVP

> Command bar for your computer agent.

The user should never have to prepare the AI before asking for work. No projects, agent pickers, skill pickers, or manual workspace setup.

## Phase 12 (current)

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
