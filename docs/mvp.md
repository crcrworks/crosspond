# Crosspond MVP

> Command bar for your computer agent.

The user should never have to prepare the AI before asking for work. No projects, agent pickers, skill pickers, or manual workspace setup.

## Phase 11 (current)

Whole-Mac agent: pick any app, type and scroll, approved shell/URL, EventKit calendar reads, and approved external file reads — on top of Phase 6 web tools and earlier computer use.

### Capability phases (7–11)

7. **Retarget any app** — `list_apps`, `open_app`, `focus_app`; optional `app` on snapshot / screenshot / UI actions. Prefer opening the right app over web search or workspace browsing for local tasks.
8. **Keyboard + scroll** — `ui_type`, `ui_hotkey`, `ui_scroll` (cua-driver wrappers; ComputerAction approval).
9. **Shell + URL** — `run_command` (workspace cwd, always Allow); `open_url` (public http(s) auto after SSRF checks; other schemes Allow).
10. **EventKit** — `calendar_events` for reading today’s (or a range of) events; prefer over Calendar.app UI scraping.
11. **External file read** — `read_file` / `list_directory` outside the workspace after one Allow (same card as external write).

Demo (Phase 10+): Option + Space with Safari in front → “カレンダーから今日の予定を確認して” → `calendar_events` → short answer. No web search, no workspace `list_directory`.

Phases 1–6 still apply (BYOK chat, workspace FS, ambient context, Accessibility, screenshot/click, web_search / fetch_url).

## Later phases (do not implement yet)

12. Polish (receipts UI, history, onboarding)
13. Release (signing, notarization, license re-audit)

## Non-goals for MVP

Projects, multi-agent orchestration, plugin/MCP marketplace, cloud accounts, Windows/Linux, local LLM runtime, drag, scheduled agents, voice, exposing cua-driver’s full MCP catalog, `kill_app`.
