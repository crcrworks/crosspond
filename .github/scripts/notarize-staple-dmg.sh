#!/usr/bin/env bash
# Notarize and staple the distributable DMG, then replace the draft Release asset.
set -euo pipefail

tag="${1:?usage: notarize-staple-dmg.sh <tag> [target-triple]}"
triple="${2:-aarch64-apple-darwin}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
eval "$(bash "$root/.github/scripts/macos-bundle-paths.sh" "$triple" "$root")"

echo "Submitting $DMG to Apple notarization"
if [[ -n "${APPLE_API_KEY_PATH:-}" && -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" ]]; then
	xcrun notarytool submit "$DMG" --wait \
		--key "$APPLE_API_KEY_PATH" \
		--key-id "$APPLE_API_KEY" \
		--issuer "$APPLE_API_ISSUER"
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
	xcrun notarytool submit "$DMG" --wait \
		--apple-id "$APPLE_ID" \
		--password "$APPLE_PASSWORD" \
		--team-id "$APPLE_TEAM_ID"
else
	echo "Notarization credentials are required: APPLE_API_KEY_PATH + APPLE_API_KEY + APPLE_API_ISSUER, or APPLE_ID + APPLE_PASSWORD + APPLE_TEAM_ID." >&2
	exit 1
fi

echo "Stapling notarization ticket onto $DMG"
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"

echo "Replacing draft Release DMG with stapled file"
gh release upload "$tag" "$DMG" --clobber
