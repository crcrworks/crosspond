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
7. Try **Summarize this**, **What shipped in Rust 1.96?** (needs Exa key), **カレンダーから今日の予定を確認して** (`calendar_events`), **Press the Continue button** in a native app, **PDF のスキル入れて** (`skill_search` / `skill_install` into `~/.crosspond/skills`; a host scan refuses malicious skills even in Auto), or ask to click something visible in a browser page. UI actions, shell, external files, and a new browser site show an **Allow** / **Cancel** card first unless the chip is **Auto**. Auto still asks before unsandboxed shell and before sending private task data to the network. Auto does not add new sites to Allowed Sites. After a task, the summary stays in the conversation and artifacts get **Show in Finder**; **History** opens a past conversation so you can follow up.

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

Pull requests and pushes to `main` run `.github/workflows/ci.yml` (fmt, portable clippy/test, macOS Seatbelt tests, UI check/test/build, `cargo audit`, `npm audit --audit-level=high`). On Linux, clippy/test skip `crosspond-macos` stubs. `gitleaks` is advisory.

## Release

Ship from `main` with the same two-step flow as other crcrworks apps:

1. `/pre-release-review` — local `cargo` / `npm` plus Bugbot and Security Review
2. `/prepare-release` — starts **Prepare Release PR**, then fill `PUBLIC_CHANGELOG.md` (do not commit from the skill)
3. After you commit the changelog and the PR is ready: `/release` — squash-merges if needed and starts **Publish Release**

Publish creates a **draft** `vX.Y.Z` GitHub Release first. CI signs the `.app`, notarizes and staples the distributable `.dmg`, re-uploads that stapled DMG, verifies Gatekeeper and Release assets, and only then publishes. Users never see `/releases/latest` (or `latest.json`) until that last step. The launcher checks that public Release when the window is shown and offers **Update available**. `tauri dev` does not check for updates.

The repo must be **public** (or Releases must be downloadable without auth) or in-app updates cannot fetch `latest.json`.

Prepare Release PR and Publish Release run only from **main** and refuse anyone who is not a repository **admin**. Apple / updater secrets are injected only into the macOS **bundle** job via GitHub Environment **`release`**.

Crosspond is licensed under **MIT OR Apache-2.0**. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE). The macOS Bundle ID is `com.crosspond.app`.

### GitHub Environment `release`

The environment **`release`** is already on the repo. Confirm Settings → Environments → **`release`**:

- Required reviewers: you (so a write collaborator’s run waits instead of signing)
- Deployment branches: **`main` only**
- Allow administrators to bypass configured protection rules: on (so you are not stuck approving your own `/release`)

Add the secrets below on **that environment**, not as ordinary repository secrets. The personal Developer ID is for binaries **you** publish; forks must use their own certificate.

### Environment secrets

Unsigned builds must not be published. Not every secret is required in every setup:

**Updater** — the matching public key is already in `crates/crosspond-app/tauri.conf.json`. The private key lives only on the machine that created it (`~/.tauri/crosspond.key`):

- `TAURI_SIGNING_PRIVATE_KEY` — required; contents of `~/.tauri/crosspond.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — only if that private key has a password

If that file is gone, generate a new pair with `npx --prefix ui tauri signer generate -w ~/.tauri/crosspond.key --ci` and replace the `plugins.updater.pubkey` value.

**Apple Developer ID** — required; export a **Developer ID Application** `.p12`:

- `APPLE_CERTIFICATE` — base64 of the `.p12` (`base64 -i certificate.p12 | pbcopy`)
- `APPLE_CERTIFICATE_PASSWORD` — `.p12` password
- `APPLE_SIGNING_IDENTITY` — optional; Tauri can infer it from the certificate
- `APPLE_TEAM_ID` — required when notarizing with Apple ID

**Notarization** — pick one complete set (App Store Connect API key **or** Apple ID):

- App Store Connect API key: `APPLE_API_KEY` (key id), `APPLE_API_ISSUER`, `APPLE_API_KEY_P8` (the `.p8` file body)
- or `APPLE_ID` plus an app-specific `APPLE_PASSWORD` and `APPLE_TEAM_ID`

Optional repository secret (not environment): `RELEASE_PLEASE_TOKEN` (a PAT) if `github.token` cannot open the release PR.

## Bundle notes

`resources/macos/Info.plist` is the accessory-app manifest (`LSUIElement`). The Tauri host also sets Accessory at runtime so `cargo run` / `tauri dev` do not steal the frontmost app (tao defaults to Regular). `tauri build` produces a `.dmg` and updater archive, copies `crosspond-chrome-host` next to the app binary, puts `extension/chrome` in `Contents/Resources/chrome-extension`, and copies `LICENSE-MIT` / `LICENSE-APACHE` to `Contents/Resources/licenses`. Settings → Browser still uses Load unpacked; the path is inside the app bundle.
