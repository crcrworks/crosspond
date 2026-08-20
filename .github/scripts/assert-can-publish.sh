#!/usr/bin/env bash
set -euo pipefail

node --input-type=module <<'NODE'
import { execFileSync } from "node:child_process";
import { canStartPublish } from "./.github/scripts/github.mjs";

const version = process.env.VERSION;
const tagName = process.env.TAG_NAME;

function ghJson(args) {
	return JSON.parse(execFileSync("gh", args, { encoding: "utf8" }));
}

let existingRelease = null;
try {
	const viewed = ghJson(["release", "view", tagName, "--json", "id,isDraft,tagName"]);
	existingRelease = { draft: viewed.isDraft === true };
} catch {
	existingRelease = null;
}

const remoteTag = execFileSync(
	"git",
	["ls-remote", "--tags", "--refs", "origin", `refs/tags/${tagName}`],
	{ encoding: "utf8" },
).trim();
if (remoteTag && !existingRelease) {
	throw new Error(
		`Tag ${tagName} exists on origin without a GitHub Release. Delete the leftover tag before retrying.`,
	);
}

const listed = ghJson(["release", "list", "--limit", "50", "--json", "tagName,isDraft"]);
const publishedVersions = listed
	.filter((row) => row.isDraft !== true)
	.map((row) => /^v([0-9]+\.[0-9]+\.[0-9]+)$/.exec(row.tagName)?.[1])
	.filter(Boolean);

canStartPublish({ requestedVersion: version, existingRelease, publishedVersions });
if (existingRelease?.draft) {
	console.log(`Draft ${tagName} already exists and will be reused.`);
}
NODE
