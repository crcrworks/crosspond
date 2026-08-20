import { pathToFileURL } from "node:url";
import {
	assertExactSemver,
	github,
	parseReleasePleasePrs,
	readRepoFile,
	releasePrNumber,
	requireEnv,
	writeRepoFile,
} from "./github.mjs";
import { upsertPublicChangelogTemplate } from "./public-changelog.mjs";

const ownerRepo = process.env.GITHUB_REPOSITORY;
const token = process.env.GITHUB_TOKEN;
const releasePleasePrs = process.env.RELEASE_PLEASE_PRS;
const releaseVersion = process.env.RELEASE_VERSION;
const releaseDate = process.env.RELEASE_DATE;
const publicChangelogPath = "PUBLIC_CHANGELOG.md";

async function main() {
	const ownerRepoValue = requireEnv(ownerRepo, "GITHUB_REPOSITORY");
	requireEnv(token, "GITHUB_TOKEN");
	const version = requireEnv(releaseVersion, "RELEASE_VERSION");
	assertExactSemver(version);

	const [owner, repo] = ownerRepoValue.split("/");
	const parsedPrs = parseReleasePleasePrs(releasePleasePrs);
	if (parsedPrs.length === 0) {
		console.log(
			"Release Please did not create or update a release PR; skipping public changelog template.",
		);
		return;
	}

	const prNumber = releasePrNumber(parsedPrs);
	const pr = await github(`/repos/${owner}/${repo}/pulls/${prNumber}`, token);
	const branch = pr.head.ref;
	const publicChangelogFile = await readRepoFile(owner, repo, publicChangelogPath, branch, token);
	const next = upsertPublicChangelogTemplate(publicChangelogFile.content, version, releaseDate);

	if (next === publicChangelogFile.content) {
		console.log(`${publicChangelogPath} already has a v${version} section; leaving it unchanged.`);
		return;
	}

	await writeRepoFile(
		owner,
		repo,
		publicChangelogPath,
		branch,
		next,
		publicChangelogFile.sha,
		`docs: add public changelog template for v${version}`,
		token,
	);

	console.log(`Added v${version} public changelog template to ${publicChangelogPath} on ${branch}.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
	main().catch((error) => {
		console.error(error);
		process.exit(1);
	});
}
