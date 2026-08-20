import assert from "node:assert/strict";
import test from "node:test";
import {
	extractPublicReleaseNotes,
	isPublicChangelogFilled,
	publicChangelogTemplate,
	upsertPublicChangelogTemplate,
} from "./public-changelog.mjs";

test("upserts a version heading at the top", () => {
	const existing = "# 更新履歴\n\n## v0.0.1 (2026-01-01)\n\n### Fixes\n\n- 最初のリリースです\n";
	const next = upsertPublicChangelogTemplate(existing, "0.1.0", "2026-08-20");
	assert.match(next, /^## v0\.1\.0 \(2026-08-20\)$/m);
	assert.ok(next.indexOf("## v0.1.0") < next.indexOf("## v0.0.1"));
});

test("leaves an existing version heading unchanged", () => {
	const existing = publicChangelogTemplate("0.1.0", "2026-08-20");
	assert.equal(upsertPublicChangelogTemplate(existing, "0.1.0", "2026-08-21"), existing);
});

test("extracts one release section", () => {
	const changelog = [
		"# 更新履歴",
		"",
		"## v0.1.0 (2026-08-20)",
		"",
		"### Features",
		"",
		"- アプリ内から更新できるようになりました",
		"",
		"## v0.0.1 (2026-01-01)",
		"",
		"### Fixes",
		"",
		"- 最初のリリースです",
		"",
	].join("\n");
	const notes = extractPublicReleaseNotes(changelog, "0.1.0");
	assert.match(notes, /アプリ内から更新/);
	assert.doesNotMatch(notes, /最初のリリース/);
	assert.equal(isPublicChangelogFilled(notes), true);
	assert.equal(isPublicChangelogFilled(publicChangelogTemplate("0.1.0", "2026-08-20")), false);
});
