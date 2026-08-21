import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { assertExactSemver, requireEnv } from "./github.mjs";
import { extractPublicReleaseNotes, isPublicChangelogFilled } from "./public-changelog.mjs";

const version = process.env.RELEASE_VERSION;
const changelogPath = process.env.PUBLIC_CHANGELOG_PATH ?? "PUBLIC_CHANGELOG.md";

function main() {
	const requested = requireEnv(version, "RELEASE_VERSION");
	assertExactSemver(requested);
	const changelog = readFileSync(changelogPath, "utf8");
	const notes = extractPublicReleaseNotes(changelog, requested);
	if (!notes || !isPublicChangelogFilled(notes)) {
		throw new Error(
			`PUBLIC_CHANGELOG.md is missing filled notes for v${requested}. Fill Features / Fixes (and drop empty sections) before publishing.`,
		);
	}
	process.stdout.write(notes);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
	try {
		main();
	} catch (error) {
		console.error(error);
		process.exit(1);
	}
}
