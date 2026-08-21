#!/usr/bin/env bash
set -euo pipefail

# Resolve from this file so callers can use any cwd (Tauri beforeBuildCommand
# runs with cwd at the repo root).
root="$(cd "$(dirname "$0")/.." && pwd)"
profile="debug"
cargo_flags=()
if [[ "${1:-}" == "--release" ]]; then
	profile="release"
	cargo_flags+=(--release)
fi

host_triple="$(rustc -vV | sed -n 's/^host: //p')"
triple="${TAURI_ENV_TARGET_TRIPLE:-$host_triple}"
if [[ -z "$triple" ]]; then
	echo "could not determine the Rust target triple" >&2
	exit 1
fi

# When Tauri sets the app target, build the sidecar for that triple so an
# x86_64 host cannot stage an Intel binary as aarch64-apple-darwin.
if [[ -n "${TAURI_ENV_TARGET_TRIPLE:-}" ]]; then
	cargo_flags+=(--target "$TAURI_ENV_TARGET_TRIPLE")
fi

cargo build -p crosspond-chrome-host --manifest-path "$root/Cargo.toml" "${cargo_flags[@]}"

target_dir="$(
	cargo metadata --format-version 1 --no-deps --manifest-path "$root/Cargo.toml" |
		python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
)"

if [[ -n "${TAURI_ENV_TARGET_TRIPLE:-}" ]]; then
	src="$target_dir/$TAURI_ENV_TARGET_TRIPLE/$profile/crosspond-chrome-host"
else
	src="$target_dir/$profile/crosspond-chrome-host"
fi
if [[ ! -f "$src" ]]; then
	echo "crosspond-chrome-host was not built at $src" >&2
	exit 1
fi

dest_dir="$root/crates/crosspond-app/binaries"
mkdir -p "$dest_dir"
dest="$dest_dir/crosspond-chrome-host-${triple}"
cp "$src" "$dest"
chmod +x "$dest"
