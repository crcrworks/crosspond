#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
profile="debug"
cargo_flags=()
if [[ "${1:-}" == "--release" ]]; then
	profile="release"
	cargo_flags+=(--release)
fi

cargo build -p crosspond-chrome-host --manifest-path "$root/Cargo.toml" "${cargo_flags[@]}"

triple="${TAURI_ENV_TARGET_TRIPLE:-$(rustc -vV | sed -n 's/^host: //p')}"
if [[ -z "$triple" ]]; then
	echo "could not determine the Rust target triple" >&2
	exit 1
fi

target_dir="$(
	cargo metadata --format-version 1 --no-deps --manifest-path "$root/Cargo.toml" |
		python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
)"
src="$target_dir/$profile/crosspond-chrome-host"
if [[ ! -f "$src" ]]; then
	echo "crosspond-chrome-host was not built at $src" >&2
	exit 1
fi

dest_dir="$root/crates/crosspond-app/binaries"
mkdir -p "$dest_dir"
dest="$dest_dir/crosspond-chrome-host-${triple}"
cp "$src" "$dest"
chmod +x "$dest"
