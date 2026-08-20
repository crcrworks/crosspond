#!/usr/bin/env bash
# Fail unless this run is on main and GITHUB_ACTOR is a repository admin.
# Release workflows must not run for write/maintain collaborators.
set -euo pipefail

ref="${GITHUB_REF:?GITHUB_REF is required}"
if [[ "${ref}" != "refs/heads/main" ]]; then
	echo "Run this workflow from main (got ${ref})." >&2
	exit 1
fi

repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
actor="${GITHUB_ACTOR:?GITHUB_ACTOR is required}"
owner="${repo%%/*}"

permission=""
if permission="$(gh api "repos/${repo}/collaborators/${actor}/permission" --jq .permission)"; then
	if [[ "${permission}" == "admin" ]]; then
		echo "Actor ${actor} has admin permission on ${repo}."
		exit 0
	fi
	echo "Only repository admins can run this workflow (${actor} has ${permission})." >&2
	exit 1
fi

# Personal-account fallback when the permission API is unavailable to GITHUB_TOKEN.
if [[ "${actor}" == "${owner}" ]]; then
	echo "Collaborator permission API unavailable; allowing repository owner ${actor}."
	exit 0
fi

echo "Only repository admins can run this workflow (could not read permission for ${actor})." >&2
exit 1
