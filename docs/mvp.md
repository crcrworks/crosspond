# Crosspond MVP

> Command bar for your computer agent.

The user should never have to prepare the AI before asking for work. No projects, agent pickers, skill pickers, or manual workspace setup.

## Phase 3 (current)

Ambient context so “this” works:

- Frontmost app name and bundle id
- Focused window title
- Selected text
- Finder selected files (copied into the task `input/` directory)
- Context badges on the launcher
- Collect on hotkey *before* Crosspond becomes frontmost

Demo: select text in Notes or files in Finder → Option + Space → “Summarize this”. The launcher shows a badge such as `Safari` / `Selected text: 428 chars` or `Finder` / `3 selected files`.

Phase 2 filesystem tools and Phase 1 BYOK chat still apply.

## Later phases (do not implement yet)

4. Accessibility computer use + approvals
5. Screenshot / vision
6. Polish (receipts UI, history, onboarding)
7. Release (signing, notarization, license re-audit)

## Non-goals for MVP

Projects, multi-agent orchestration, plugin/MCP marketplace, cloud accounts, Windows/Linux, local LLM runtime, pixel-coordinate computer use, scheduled agents, voice.
