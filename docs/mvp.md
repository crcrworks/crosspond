# Crosspond MVP

> Command bar for your computer agent.

The user should never have to prepare the AI before asking for work. No projects, agent pickers, skill pickers, or manual workspace setup.

## Phase 6 (current)

Look things up on the public web without driving a browser.

- `web_search` — Exa Search API (title / URL / snippet); key in Keychain via Settings
- `fetch_url` — read a public http(s) page as text (SSRF-blocked); no approval card
- Prefer these for research; Accessibility / screenshot click remain for UI work

Demo: Option + Space → “What shipped in Rust 1.96?” → web_search → short answer with links.

Phase 5 screenshot/click, Phase 4 Accessibility, Phase 3 ambient context, Phase 2 filesystem tools, and Phase 1 BYOK chat still apply.

## Later phases (do not implement yet)

7. Polish (receipts UI, history, onboarding)
8. Release (signing, notarization, license re-audit)

## Non-goals for MVP

Projects, multi-agent orchestration, plugin/MCP marketplace, cloud accounts, Windows/Linux, local LLM runtime, drag / scroll / synthetic keyboard, scheduled agents, voice.
