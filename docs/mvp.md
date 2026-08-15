# Crosspond MVP

> Command bar for your computer agent.

The user should never have to prepare the AI before asking for work. No projects, agent pickers, skill pickers, or manual workspace setup.

## Phase 5 (current)

See the frontmost app and click in the image when Accessibility is not enough.

- `take_screenshot` / `ui_click` — vision fallback when Accessibility has no useful label
- Prefer `get_accessibility_snapshot` + `ui_press` for named controls; `ui_press` clicks the node through cua-driver
- UI actions: **Manual** (default) asks Allow; **Auto** runs them; **AI** lets the model set `ask_user`
- Cycle the mode with the chip next to the prompt
- Requires [cua-driver](https://cua.ai/cua-driver) on `PATH` (or `CUA_DRIVER_BIN`)
- Phase 4 Accessibility tools: `get_accessibility_snapshot`, `ui_press`, `ui_set_value`

Demo: Helium in front → Option + Space → “Press Login on this page” → screenshot → Allow → click.

Phase 4 Accessibility, Phase 3 ambient context, Phase 2 filesystem tools, and Phase 1 BYOK chat still apply.

## Later phases (do not implement yet)

6. Polish (receipts UI, history, onboarding)
7. Release (signing, notarization, license re-audit)

## Non-goals for MVP

Projects, multi-agent orchestration, plugin/MCP marketplace, cloud accounts, Windows/Linux, local LLM runtime, drag / scroll / synthetic keyboard, scheduled agents, voice.
