#!/usr/bin/env bash
# Fail unless Apple Developer ID + notarization + updater signing secrets are present.
# Public GitHub Releases must not ship an unsigned .dmg.
set -euo pipefail

if [[ -z "${APPLE_CERTIFICATE:-}" || -z "${APPLE_CERTIFICATE_PASSWORD:-}" ]]; then
	echo "APPLE_CERTIFICATE and APPLE_CERTIFICATE_PASSWORD are required to sign a public macOS release." >&2
	exit 1
fi

api_key=0
if [[ -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_KEY_P8:-}" ]]; then
	api_key=1
fi
apple_id=0
if [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
	apple_id=1
fi
if [[ "$api_key" -eq 0 && "$apple_id" -eq 0 ]]; then
	echo "Notarization credentials are required: APPLE_API_ISSUER + APPLE_API_KEY + APPLE_API_KEY_P8, or APPLE_ID + APPLE_PASSWORD + APPLE_TEAM_ID." >&2
	exit 1
fi

if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
	echo "TAURI_SIGNING_PRIVATE_KEY must be set so updater artifacts can be signed." >&2
	exit 1
fi
