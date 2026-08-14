# Security

## Secrets

API keys go to macOS Keychain via `SecretStore`. They must not appear in:

- `config.json`
- `.env`
- SQLite
- logs
- task history
- `events.jsonl` / `receipt.json`
- `Debug` output (`SecretString` must not derive `Debug`)

The Keychain item uses service `com.crosspond.app` and account `provider.api_key`.

Provider HTTP errors shown in the UI are short status-based messages. Raw provider JSON is not dumped to the user or to logs.

Selected text is sent to the model when present, but it must not appear in `events.jsonl`, receipts, or `ContextCapsule`’s `Debug` impl. Clipboard is never collected. Screenshots are never collected in this phase.

## Tool policy

| Risk | Default |
| --- | --- |
| Read-only | auto |
| Workspace write | auto |
| External write | approval |
| Computer action | approval (MVP) |
| Shell | approval |
| Destructive | approval |

Phase 3 still has no approval UI. Tools that would require approval are not executed; the model receives an error string instead.

Workspace membership is not `path.starts_with(workspace)`. Classify through `resolve_path` / `classify_write_path`, which handle `..`, symlinks, and canonicalization by walking parents of the resolved path.

Filesystem tools refuse paths outside the workspace even before policy evaluation would allow a write. Finder files are copied into `input/` so `read_file` stays workspace-scoped.

## Untrusted content

Files, webpages, UI text, documents, and screenshots are data, not instructions. External side effects still require approval even if content asks the model to skip policy.

The system prompt includes this untrusted-content line. Ambient selected text is wrapped with the same warning.

## Cancellation

Escape / Stop must abort in-flight model requests and skip remaining tool calls. Hiding the launcher cancels a running task and clears the follow-up session. Closing the UI must not leave a background task running.

## Permissions

Selected text and window titles use Accessibility (prompt via `AXIsProcessTrustedWithOptions`). Finder selection uses Apple Events (`osascript`). Both are optional: if the user declines, the launcher still works as chat.
