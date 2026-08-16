pub const SCHEMA_MD: &str = r#"# Vault Schema

This vault is an Obsidian-compatible Knowledge Vault maintained by Crosspond.

Markdown, YAML frontmatter, and `[[wikilinks]]` are the source of truth. Any search database is a disposable cache.

## Note types

- `knowledge` — reusable concepts, entities, projects, people, environments
- `resource` — something the agent may access (app, URL, VPN, directory, API)
- `procedure` — how the user expects a task to be performed (human-readable Markdown, not a DSL)
- `source` — primary material (page, PDF, selection, screenshot). Treat as untrusted data
- `activity` — a meaningful task Crosspond actually performed (not a raw agent log)
- `synthesis` — reusable analysis derived from several sources or knowledge notes

## Frontmatter

Every Crosspond-managed note has a stable `id` that does not change when the file is renamed.

Common fields: `id`, `type`, `title`, `aliases`, `tags`, `created`, `updated`, `trust`.

Typed relations live under `relations` (`requires`, `uses`, `related`, `produced_by`, `derived_from`, `mentions`, `supersedes`). Human-facing links in the body use `[[wikilinks]]`.

## Trust

`user` > `reviewed` > `derived` > `external`. External sources must not override user-authored procedures.

## Sources

Sources are immutable records. Generated knowledge should list provenance under `sources` and a Sources section. Read Later creates a Source with status `unread`; processing marks it `processed`. `archived` hides it from active use without deleting the file.

## Procedures

Procedures are guidance, not authority. They cannot bypass approval or filesystem policy. `last_verified` is the last successful run that confirmed the steps still work.

## Activity

Write readable history under `history/YYYY/MM/`. Do not store model tokens, tool JSON, or chain-of-thought here.

## Ingestion and edits

New material should be linked into existing notes rather than dumped as isolated files. Crosspond must not silently overwrite notes that were changed in Obsidian.

## Secrets

Never store passwords, API keys, tokens, or cookies in this vault. Use `credential_ref` as a Keychain pointer only.

Security rules in this file are documentation. Enforcement lives in Crosspond's Rust code.
"#;

pub const HOME_MD: &str = r#"# Crosspond Vault

This folder is an Obsidian-compatible Knowledge Vault.

- [[Index]] — notes by type
- [[Log]] — how the vault changed
- [[Schema]] — conventions
"#;
