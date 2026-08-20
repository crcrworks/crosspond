#!/usr/bin/env bash
# Verify signed/notarized macOS artifacts after the stapled DMG is re-uploaded.
set -euo pipefail

tag="${1:?usage: verify-macos-release.sh <tag> [target-triple]}"
triple="${2:-aarch64-apple-darwin}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
eval "$(bash "$root/.github/scripts/macos-bundle-paths.sh" "$triple" "$root")"

echo "Verifying code signature of $APP"
codesign --verify --deep --strict --verbose=2 "$APP"

echo "Assessing Gatekeeper on $APP"
spctl --assess --type execute -vv "$APP"

echo "Validating notarization ticket on $DMG"
xcrun stapler validate "$DMG"

local_name="$(basename "$DMG")"
local_size="$(stat -f%z "$DMG")"
local_digest="$(shasum -a 256 "$DMG" | awk '{print $1}')"

asset_json="$(gh release view "$tag" --json assets,isDraft)"
ASSET_JSON="$asset_json" LOCAL_NAME="$local_name" LOCAL_SIZE="$local_size" \
	node --input-type=module <<'NODE'
import { assertDraftReleaseAssets } from "./.github/scripts/github.mjs";

const payload = JSON.parse(process.env.ASSET_JSON);
const names = assertDraftReleaseAssets({
	isDraft: payload.isDraft,
	assets: payload.assets,
	localDmgName: process.env.LOCAL_NAME,
	localDmgSize: process.env.LOCAL_SIZE,
});
console.log("draft release assets ok:", names.join(", "));
console.log(`stapled DMG size matches GitHub asset (${process.env.LOCAL_SIZE} bytes)`);
NODE

download_dir="$(mktemp -d)"
trap 'rm -rf "$download_dir"' EXIT
gh release download "$tag" --pattern "$local_name" --dir "$download_dir"
remote_digest="$(shasum -a 256 "$download_dir/$local_name" | awk '{print $1}')"
if [[ "$remote_digest" != "$local_digest" ]]; then
	echo "draft DMG digest $remote_digest does not match stapled local digest $local_digest" >&2
	exit 1
fi
echo "stapled DMG digest matches GitHub asset ($local_digest)"
