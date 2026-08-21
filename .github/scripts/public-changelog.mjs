import { assertExactSemver } from "./github.mjs";

const releaseSectionTitles = ["Features", "Improvements", "Changes", "Fixes", "Developments"];

export function normalizedDate(date) {
	if (date && /^[0-9]{4}-[0-9]{2}-[0-9]{2}$/.test(date)) {
		return date;
	}
	return new Date().toISOString().slice(0, 10);
}

export function publicChangelogTemplate(version, date) {
	assertExactSemver(version);
	const sections = releaseSectionTitles
		.map(
			(title) =>
				`### ${title}\n\n<!-- Add public-facing ${title.toLowerCase()} bullet items here. -->`,
		)
		.join("\n\n");
	return `## v${version} (${normalizedDate(date)})\n\n${sections}`;
}

function escapeRegExp(value) {
	return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function findReleaseBlock(changelog, version) {
	const headingPattern = new RegExp(
		`^##\\s+~?v?${escapeRegExp(version)}(?:\\s+\\([^)]*\\))?\\s*$`,
		"im",
	);
	const heading = headingPattern.exec(changelog);
	if (heading?.index == null) {
		return null;
	}

	const start = heading.index;
	const afterHeading = start + heading[0].length;
	const nextReleaseRelative = changelog.slice(afterHeading).search(/\n## .+$/m);
	const end = nextReleaseRelative === -1 ? changelog.length : afterHeading + nextReleaseRelative;

	return {
		block: changelog.slice(start, end),
		end,
		start,
	};
}

export function upsertPublicChangelogTemplate(changelog, version, date) {
	assertExactSemver(version);

	const existingRelease = findReleaseBlock(changelog, version);
	if (existingRelease) {
		return changelog;
	}

	const template = publicChangelogTemplate(version, date);
	const firstRelease = /^## .+$/m.exec(changelog);
	if (firstRelease?.index == null) {
		return `${changelog.trimEnd()}\n\n${template}\n`;
	}

	const before = changelog.slice(0, firstRelease.index).trimEnd();
	const after = changelog.slice(firstRelease.index).replace(/^\n+/, "");
	return `${before}\n\n${template}\n\n${after}`;
}

export function extractPublicReleaseNotes(changelog, version) {
	assertExactSemver(version);
	const block = findReleaseBlock(changelog, version);
	if (!block) {
		return null;
	}
	return `${block.block.trim()}\n`;
}

export function isPublicChangelogFilled(section) {
	if (!section) {
		return false;
	}
	const bullets = section.match(/^[-*] .+$/gm) ?? [];
	return bullets.some((line) => !line.includes("Add public-facing"));
}
