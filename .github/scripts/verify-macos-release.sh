#!/usr/bin/env bash
# Verify signed/notarized macOS artifacts before a draft GitHub Release is published.
set -euo pipefail

tag="${1:?usage: verify-macos-release.sh <tag> [target-triple]}"
triple="${2:-aarch64-apple-darwin}"
root="$(cd "$(dirname "$0")/../.." && pwd)"

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

echo "Verifying code signature of $app"
codesign --verify --deep --strict --verbose=2 "$app"

echo "Assessing Gatekeeper on $app"
spctl --assess --type execute -vv "$app"

echo "Validating notarization ticket on $dmg"
xcrun stapler validate "$dmg"

asset_json="$(gh release view "$tag" --json assets,isDraft)"
python3 - "$asset_json" <<'PY'
import json, sys

payload = json.loads(sys.argv[1])
if payload.get("isDraft") is not True:
    raise SystemExit("release must still be a draft while verifying artifacts")
names = [asset.get("name") or "" for asset in payload.get("assets") or []]
missing = []
if not any(name.endswith(".dmg") for name in names):
    missing.append(".dmg")
if not any(name.endswith(".app.tar.gz") for name in names):
    missing.append(".app.tar.gz")
if not any(name.endswith(".app.tar.gz.sig") for name in names):
    missing.append(".app.tar.gz.sig")
if "latest.json" not in names:
    missing.append("latest.json")
if missing:
    raise SystemExit(f"draft release is missing assets: {', '.join(missing)} (have {names})")
print("draft release assets ok:", ", ".join(names))
PY
