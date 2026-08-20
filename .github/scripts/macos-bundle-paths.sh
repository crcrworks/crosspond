#!/usr/bin/env bash
# Print APP/DMG/TARBALL/SIG paths for the Tauri macOS bundle. Source-friendly:
#   eval "$(bash macos-bundle-paths.sh <triple> <repo-root>)"
set -euo pipefail

triple="${1:?usage: macos-bundle-paths.sh <target-triple> <repo-root>}"
root="${2:?}"

target_dir="$(
	cargo metadata --format-version 1 --no-deps --manifest-path "$root/Cargo.toml" |
		python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
)"
bundle="$target_dir/$triple/release/bundle"
macos_dir="$bundle/macos"
dmg_dir="$bundle/dmg"

shopt -s nullglob
apps=("$macos_dir"/*.app)
dmgs=("$dmg_dir"/*.dmg)
tarballs=("$macos_dir"/*.app.tar.gz)

if [[ "${#apps[@]}" -ne 1 ]]; then
	echo "expected one .app under $macos_dir, found ${#apps[@]}" >&2
	exit 1
fi
if [[ "${#dmgs[@]}" -ne 1 ]]; then
	echo "expected one .dmg under $dmg_dir, found ${#dmgs[@]}" >&2
	exit 1
fi
if [[ "${#tarballs[@]}" -ne 1 ]]; then
	echo "expected one .app.tar.gz under $macos_dir, found ${#tarballs[@]}" >&2
	exit 1
fi

app="${apps[0]}"
dmg="${dmgs[0]}"
tarball="${tarballs[0]}"
sig="${tarball}.sig"
if [[ ! -f "$sig" ]]; then
	echo "missing updater signature $sig" >&2
	exit 1
fi

printf 'APP=%q\n' "$app"
printf 'DMG=%q\n' "$dmg"
printf 'TARBALL=%q\n' "$tarball"
printf 'SIG=%q\n' "$sig"
