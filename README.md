# Crosspond

Command bar for your computer agent. macOS only.

Phase 13 adds a Chrome extension so Chromium pages use DOM snapshots and refs (`browser_snapshot` / `browser_click`) instead of Accessibility or screenshots. Native apps still use the Phase 11 Whole-Mac tools. Phase 12 receipts, history, and onboarding stay. Settings can sign in with ChatGPT Plus/Pro and keep several OpenAI Compatible endpoints; pick the model in the launcher.

## Run

```bash
# Computer use needs cua-driver on PATH (https://cua.ai/cua-driver).
# Override with CUA_DRIVER_BIN if it is installed somewhere else.
npm --prefix ui install
npm --prefix ui run desktop
```

That runs the Tauri 2 host (`crates/crosspond-app`) and the SvelteKit Vite server together. To run them separately: `npm --prefix ui run dev` then `cargo build -p crosspond-chrome-host && cargo run -p crosspond-app`. `npm --prefix ui run check` and `npm --prefix ui test` cover the frontend. `cargo build` at the workspace root also builds `crosspond-chrome-host`, which Chrome launches for native messaging.

Then:

1. On first launch, Crosspond opens a welcome. Press **⌘,** (or **Open Settings**) and open the **Models** tab: sign in with ChatGPT Plus/Pro and/or add one or more OpenAI Compatible endpoints (name, Base URL, API key). You can use both at once. Optionally add an Exa API key on the **Search** tab. The Knowledge Vault path defaults to `~/Documents/Crosspond` (**Knowledge** tab).
2. Click **Save** / **Test** on each provider, then **Continue**. On **Crosspond is ready**, click **Open** (or press the launcher shortcut) to show the command bar. Pick the model (and ChatGPT effort) under the launcher prompt. If port 1455 is already in use (often Codex CLI), paste the ChatGPT redirect URL to finish sign-in.
3. Grant Accessibility, Screen Recording, or Calendars later from Settings → Permissions, or when a tool needs them. Finder selection may also prompt for Automation.
4. Select text in another app, or files in Finder — or leave Safari / Chrome / Helium in front.
5. Press **Option + Space** (change this in Settings) to show the bar from any app. A badge should show the app and “Selected text: N chars” or “N selected files”.
6. Keep Crosspond running so it can register `com.crosspond.chrome`. Then load the unpacked extension from Settings → Browser (chrome://extensions → Developer mode → Load unpacked → `extension/chrome`). If Chrome already had the extension loaded, click Reload. Settings → Browser should show **Connected**. Then ask to click something on the current page; Crosspond uses `browser_snapshot` rather than screenshots.
7. Try **Summarize this**, **What shipped in Rust 1.96?** (needs Exa key), **カレンダーから今日の予定を確認して** (`calendar_events`), **Press the Continue button** in a native app, or ask to click something visible in a browser page. UI actions, shell, external files, and a new browser site show an **Allow** / **Cancel** card first unless the chip is **Auto**. Auto does not add new sites to Allowed Sites. After a task, the summary stays in the conversation and artifacts get **Show in Finder**; **History** opens a past conversation so you can follow up.

**Escape** or **Stop** cancels the whole task while it is running. Approval **Cancel** rejects only that action. Hiding the window keeps work running and keeps the conversation; press **New** to start fresh.

If hotkey registration fails, the window opens immediately so the app is still usable.

For local OpenAI-compatible servers, Base URL must include `/v1` (for example `http://127.0.0.1:1234/v1`). An empty API key is rejected; use a dummy value such as `lm-studio`. Vision-capable models are required for screenshot tools.

## Develop

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix ui run check
npm --prefix ui test
```

## Release

Ship from `main` with the same two-step flow as other crcrworks apps:

1. `/pre-release-review` — local `cargo` / `npm` plus Bugbot and Security Review
2. `/prepare-release` — starts **Prepare Release PR**, then fill `PUBLIC_CHANGELOG.md` (do not commit from the skill)
3. After you commit the changelog and the PR is ready: `/release` — squash-merges if needed and starts **Publish Release**

Publish creates `vX.Y.Z`, a GitHub Release (the `.dmg` is the download), and updater files (`Crosspond.app.tar.gz`, `latest.json`). Apple Silicon only. The launcher checks that Release when the window is shown and offers **Update available** on the right of the header.

The repo must be **public** (or Releases must be downloadable without auth) or in-app updates cannot fetch `latest.json`.

Prepare Release PR and Publish Release run only from **main**, on the GitHub Environment **`release`**. Jobs then refuse anyone who is not a repository **admin**. Put signing secrets on that environment (not as ordinary repository secrets) so a write collaborator cannot read them from some other workflow.

### GitHub Environment `release`

The environment **`release`** is already on the repo. Confirm Settings → Environments → **`release`**:

- Required reviewers: you (so a write collaborator’s run waits instead of signing)
- Deployment branches: **`main` only**
- Allow administrators to bypass configured protection rules: on (so you are not stuck approving your own `/release`)

Add the secrets below on **that environment**, not as ordinary repository secrets.

### Environment secrets

Put these on **`release`**. Do not commit private keys. The personal Developer ID is for binaries **you** publish; forks must use their own certificate.

**Updater (required)** — the matching public key is already in `crates/crosspond-app/tauri.conf.json`. The private key generated with this change lives only on the machine that created it (`~/.tauri/crosspond.key`). Copy it into the environment:

- `TAURI_SIGNING_PRIVATE_KEY` — contents of `~/.tauri/crosspond.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — empty unless you set a password

If that file is gone, generate a new pair with `npx --prefix ui tauri signer generate -w ~/.tauri/crosspond.key --ci` and replace the `plugins.updater.pubkey` value.

**Apple signing / notarization (needed for Gatekeeper)** — create a **Developer ID Application** certificate, export a `.p12`, and add:

- `APPLE_CERTIFICATE` — base64 of the `.p12` (`base64 -i certificate.p12 | pbcopy`)
- `APPLE_CERTIFICATE_PASSWORD` — `.p12` password
- `APPLE_SIGNING_IDENTITY` — e.g. `Developer ID Application: Example Ltd (TEAMID)`
- `APPLE_TEAM_ID`

Notarization, pick one:

- App Store Connect API key: `APPLE_API_KEY` (key id), `APPLE_API_ISSUER`, `APPLE_API_KEY_P8` (the `.p8` file body)
- or `APPLE_ID` plus an app-specific `APPLE_PASSWORD`

Optional repository secret (not environment): `RELEASE_PLEASE_TOKEN` (a PAT) if `github.token` cannot open the release PR.

## Bundle notes

`resources/macos/Info.plist` is the accessory-app manifest (`LSUIElement`). The Tauri host also sets Accessory at runtime so `cargo run` / `tauri dev` do not steal the frontmost app (tao defaults to Regular). `tauri build` produces a `.dmg` and updater archive, copies `crosspond-chrome-host` next to the app binary, and puts `extension/chrome` in `Contents/Resources/chrome-extension`. Settings → Browser still uses Load unpacked; the path is inside the app bundle.
