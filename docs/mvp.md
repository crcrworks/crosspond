# Crosspond MVP

> Command bar for your computer agent.

The user should never have to prepare the AI before asking for work. No projects, agent pickers, skill pickers, or manual workspace setup.

## Phase 4 (current)

Operate the current Mac app through Accessibility — not pixels.

- `get_accessibility_snapshot` — compact AX tree with temporary node ids (auto)
- `ui_press(node_id)` / `ui_set_value(node_id, value)` — after **Allow**
- Approval card on the launcher; Escape still cancels the whole task
- Approval **Cancel** rejects that action only; the agent loop continues

Demo: Safari in front → Option + Space → “Press the Continue button” → snapshot → find the button → Allow → press.

Phase 3 ambient context, Phase 2 filesystem tools, and Phase 1 BYOK chat still apply.

## Later phases (do not implement yet)

5. Screenshot / vision
6. Polish (receipts UI, history, onboarding)
7. Release (signing, notarization, license re-audit)

## Non-goals for MVP

Projects, multi-agent orchestration, plugin/MCP marketplace, cloud accounts, Windows/Linux, local LLM runtime, pixel-coordinate computer use, scheduled agents, voice.
