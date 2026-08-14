# GPUI evaluation (Phase 0)

This is the license and feasibility gate required before later phases. It is not legal advice.

## Decision for Phase 0

**Continue internally on crates.io `gpui = "=0.2.2"`.** Do not distribute a closed-source build to other users until Phase 7 re-audits this file.

UI-only crates stay isolated so a SwiftUI/AppKit replacement remains possible if GPUI becomes blocked.

## Version pin

| Item | Value |
| --- | --- |
| Crate | `gpui` |
| Version | `=0.2.2` |
| Source | crates.io |
| Upstream git (from `.cargo_vcs_info.json`) | `69e2130295c2649963eb639fc70b4f2ee8ea1624` |
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

Limitations:

- **No per-window hide/show.** 0.2.2 only closes windows (`remove_window`) or hides the whole app. Crosspond toggles with `hide` / `activate`, matching the official example.
- **Metal toolchain.** Xcode 27 does not ship the `metal` compiler until `xcodebuild -downloadComponent MetalToolchain`. GPUI's build script fails without it.
- **Pre-1.0.** Breaking changes between crates.io and Zed `main` are routine.
- **`cargo run` is not an `.app` bundle.** `LSUIElement` in `resources/macos/Info.plist` does not apply until the binary is wrapped. Dock icon appears in development.

## License tree (default macOS `crosspond-app` build)

Commands:

```bash
cargo tree -p crosspond-app -e normal -f "{p} {l}"
cargo metadata --format-version 1
```

Results on 2026-08-14:

- `gpui 0.2.2` and the `gpui_*` crates it publishes are **Apache-2.0**.
- The default runtime graph is Apache-2.0 / MIT / BSD / ISC / Zlib / Unicode-3.0, plus the notes below.
- **`ztracing` / `zlog` (GPL-3.0-or-later) are not in this graph.** [zed#55470](https://github.com/zed-industries/zed/issues/55470) describes `gpui → sum_tree → ztracing` on Zed `main`. crates.io `gpui_sum_tree 0.2.2` depends only on `arrayvec`, `log`, and `rayon`.

Items that need attention:

| Crate | License | Role |
| --- | --- | --- |
| `option-ext 0.2.0` | MPL-2.0 | Runtime, via `zed-font-kit` → `dirs` → `dirs-sys` |
| `cbindgen 0.28.0` | MPL-2.0 | GPUI build-dependency; not linked into the app |
| `block 0.1.6` | MIT | Future-incompat warning from `objc`; not our code |

MPL-2.0 is file-level copyleft. Shipping a binary that includes `option-ext` typically means offering that crate's source (not the whole Crosspond tree). Phase 7 must record how that offer is made.

Dual-licensed crates that *mention* GPL (`self_cell` Apache-2.0 OR GPL-2.0-only, `r-efi` MIT OR Apache-2.0 OR LGPL) are **not** in the default `crosspond-app` runtime graph. If they appear later, prefer the permissive side.

## What this does *not* clear

- Closed-source App Store / notarized distribution
- Switching from crates.io 0.2.2 to a Zed git revision (re-open the `ztracing` question)
- Enabling extra GPUI features (`screen-capture`, `macos-blade`, Linux backends)

## Recommendation

1. Keep GPUI pinned to `=0.2.2` until a deliberate upgrade.
2. Keep `crosspond-core` / `model` / `tools` / `macos` free of GPUI.
3. Re-run this audit in Phase 7, and immediately if the GPUI pin changes.
4. Do not treat Apache-2.0 on the `gpui` crate as a complete answer for a commercial binary until `option-ext` (MPL) and any future GPL edges are reviewed.
