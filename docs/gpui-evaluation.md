# GPUI evaluation (historical)

Crosspond’s UI host is now **Tauri 2 + SvelteKit** (`ui/` + `crates/crosspond-app`). This file is the Phase 0 GPUI license and feasibility record from before that switch. Do not use it as current UI guidance.

Tauri (`Apache-2.0 OR MIT`), WKWebView, Svelte, and SvelteKit are permissively licensed. Re-audit the lockfile in Phase 13.

---

# GPUI evaluation (Phase 0)

This is the license and feasibility gate required before later phases. It is not legal advice.

## Decision for Phase 0

**Continue internally on crates.io `gpui = "=0.2.2"`.** Do not distribute a closed-source build to other users until Phase 13 re-audits this file.

UI-only crates stay isolated so a SwiftUI/AppKit replacement remains possible if GPUI becomes blocked.

## Version pin

| Item | Value |
| --- | --- |
| Crate | `gpui` |
| Version | `=0.2.2` |
| Source | crates.io, locally patched at `third_party/gpui` (removed after the Tauri migration) |
| Upstream git (from `.cargo_vcs_info.json`) | `69e2130295c2649963eb639fc70b4f2ee8ea1624` |
| Local patch | Drop the `MacWindowState` lock before `resignKeyWindow` ([zed#51035](https://github.com/zed-industries/zed/pull/51035)). crates.io 0.2.2 deadlocks the main thread when a PopUp becomes key. Also: PopUp IME (`windowLevel`, `acceptsFirstResponder`, `hidesOnDeactivate = NO`, re-activate `NSTextInputContext` on become-key, after `setContentSize`, and when `currentInputContext` is nil) so Japanese input keeps the candidate window and does not stick in roman-only after hide/show or the compact→conversation resize. |
| Declared license | Apache-2.0 |
| Docs / examples used | `https://docs.rs/gpui/0.2.2/` and the crate's `examples/` |

Zed `main` has already diverged (`gpui_platform::application`, extra `ShapedLine::paint` arguments). Phase 0 follows the published 0.2.2 examples, not current Zed source.

## Feasibility findings

Confirmed against 0.2.2:

- `Application::new().run`, `App::open_window`, `WindowKind::PopUp`, `titlebar: None`
- Text input via `EntityInputHandler` (`examples/input.rs`)
- Escape / Enter via `actions!` + `KeyBinding`
- App hide/show via `App::hide()` and `App::activate(true)` (`examples/window.rs`)
- Tokio can run on a dedicated thread; GPUI polls results with `Timer`

Limitations (why the UI later moved to Tauri):

- **PopUp key-status deadlock (patched).** crates.io 0.2.2 deadlocks the main thread on `resignKeyWindow`; `third_party/gpui` applied [zed#51035](https://github.com/zed-industries/zed/pull/51035).
- **PopUp Japanese IME (patched).** crates.io 0.2.2 NSPanel hides on IME candidate windows, reports no `windowLevel`, and does not restore first responder / `NSTextInputContext` after `App::hide()` or `setContentSize`.
- **Metal toolchain.** Xcode 27 does not ship the `metal` compiler until `xcodebuild -downloadComponent MetalToolchain`.
- **Pre-1.0.** Breaking changes between crates.io and Zed `main` are routine.
- **`App::hide()` hides Settings** because GPUI 0.2.2 has no per-window hide.

## License tree (default macOS `crosspond-app` build)

Commands:

```bash
cargo tree -p crosspond-app -e normal -f "{p} {l}"
cargo metadata --format-version 1
```

Results on 2026-08-14 (GPUI host):

- `gpui 0.2.2` and the `gpui_*` crates it publishes are **Apache-2.0**.
- The default runtime graph is Apache-2.0 / MIT / BSD / ISC / Zlib / Unicode-3.0, plus the notes below.
- **`ztracing` / `zlog` (GPL-3.0-or-later) are not in this graph.** [zed#55470](https://github.com/zed-industries/zed/issues/55470) describes `gpui → sum_tree → ztracing` on Zed `main`. crates.io `gpui_sum_tree 0.2.2` depends only on `arrayvec`, `log`, and `rayon`.

Items that need attention:

| Crate | License | Role |
| --- | --- | --- |
| `option-ext 0.2.0` | MPL-2.0 | Runtime, via `zed-font-kit` → `dirs` → `dirs-sys` |
| `cbindgen 0.28.0` | MPL-2.0 | GPUI build-dependency; not linked into the app |
| `block 0.1.6` | MIT | Future-incompat warning from `objc`; not our code |

MPL-2.0 is file-level copyleft. Shipping a binary that includes `option-ext` typically means offering that crate's source (not the whole Crosspond tree). Phase 13 must record how that offer is made.

Dual-licensed crates that *mention* GPL (`self_cell` Apache-2.0 OR GPL-2.0-only, `r-efi` MIT OR Apache-2.0 OR LGPL) are **not** in this graph for the Tauri host. If they appear later, prefer the permissive side.

## What this does *not* clear

- Closed-source App Store / notarized distribution
- The former GPUI pin or a Zed git revision (re-open the `ztracing` question if GPUI returns)

## Recommendation (superseded)

The GPUI host was replaced by Tauri 2 + SvelteKit. Keep `crosspond-core` / `model` / `tools` / `macos` free of UI frameworks. Re-run a license audit in Phase 13 against the Tauri lockfile.
