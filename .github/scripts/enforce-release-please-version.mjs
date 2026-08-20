import { pathToFileURL } from "node:url";
import {
	assertExactSemver,
	compareSemver,
	github,
	parseReleasePleasePrs,
	readRepoFile,
	releasePrNumber,
	requireEnv,
	writeRepoFile,
} from "./github.mjs";

const ownerRepo = process.env.GITHUB_REPOSITORY;
const token = process.env.GITHUB_TOKEN;
const releasePleasePrs = process.env.RELEASE_PLEASE_PRS;
const releaseVersion = process.env.RELEASE_VERSION;

const manifestPath = ".release-please-manifest.json";
const cargoTomlPath = "Cargo.toml";
const tauriConfPath = "crates/crosspond-app/tauri.conf.json";
const packageJsonPath = "ui/package.json";
const changelogPath = "CHANGELOG.md";

function pinCargoWorkspaceVersion(toml, version) {
	const section = toml.indexOf("[workspace.package]");
	if (section === -1) {
		throw new Error("Cargo.toml is missing [workspace.package].");
	}
	const before = toml.slice(0, section);
	const rest = toml.slice(section);
	const nextRest = rest.replace(
		/^version = "[0-9]+\.[0-9]+\.[0-9]+"/m,
		`version = "${version}"`,
	);
	if (nextRest === rest) {
		throw new Error("Could not pin Cargo.toml workspace.package.version.");
	}
	return `${before}${nextRest}`;
}

function pinChangelogVersion(changelog, ownerRepoValue, detectedVersion, requestedVersion, previousVersion) {
	const headingPattern = new RegExp(
		`^## \\[${detectedVersion.replaceAll(".", "\\.")}\\]\\([^)]*\\) \\(([^)]*)\\)$`,
		"m",
	);
	const replacement = `## [${requestedVersion}](https://github.com/${ownerRepoValue}/compare/v${previousVersion}...v${requestedVersion}) ($1)`;
	const next = changelog.replace(headingPattern, replacement);
	if (next === changelog) {
		if (detectedVersion === requestedVersion) {
			return changelog;
		}
		throw new Error(`Could not find the generated ${detectedVersion} changelog heading.`);
	}
	return next;
}

async function main() {
	const ownerRepoValue = requireEnv(ownerRepo, "GITHUB_REPOSITORY");
	requireEnv(token, "GITHUB_TOKEN");
	const requestedVersion = requireEnv(releaseVersion, "RELEASE_VERSION");
	assertExactSemver(requestedVersion);

	const [owner, repo] = ownerRepoValue.split("/");
	const parsedPrs = parseReleasePleasePrs(releasePleasePrs);
	if (parsedPrs.length === 0) {
		console.log("Release Please did not create or update a release PR; skipping version enforcement.");
		return;
	}

	const prNumber = releasePrNumber(parsedPrs);
	const pr = await github(`/repos/${owner}/${repo}/pulls/${prNumber}`, token);
	const branch = pr.head.ref;
	const baseBranch = pr.base.ref;

	const baseManifestFile = await readRepoFile(owner, repo, manifestPath, baseBranch, token);
	const baseManifest = JSON.parse(baseManifestFile.content);
	const previousVersion = baseManifest["."];
	assertExactSemver(previousVersion, ".release-please-manifest.json");
	if (compareSemver(requestedVersion, previousVersion) <= 0) {
		throw new Error(
			`Requested version ${requestedVersion} must be greater than current version ${previousVersion}.`,
		);
	}

	const manifestFile = await readRepoFile(owner, repo, manifestPath, branch, token);
	const manifest = JSON.parse(manifestFile.content);
	const detectedVersion = manifest["."];
	assertExactSemver(detectedVersion, ".release-please-manifest.json");
	manifest["."] = requestedVersion;

	const cargoFile = await readRepoFile(owner, repo, cargoTomlPath, branch, token);
	const cargoToml = pinCargoWorkspaceVersion(cargoFile.content, requestedVersion);

	const tauriFile = await readRepoFile(owner, repo, tauriConfPath, branch, token);
	const tauriConf = JSON.parse(tauriFile.content);
	tauriConf.version = requestedVersion;

	const packageFile = await readRepoFile(owner, repo, packageJsonPath, branch, token);
	const packageJson = JSON.parse(packageFile.content);
	packageJson.version = requestedVersion;

	const changelogFile = await readRepoFile(owner, repo, changelogPath, branch, token);
	const changelog = pinChangelogVersion(
		changelogFile.content,
		ownerRepoValue,
		detectedVersion,
		requestedVersion,
		previousVersion,
	);

	async function writeIfChanged(path, next, current, message) {
		if (next === current.content) {
			return;
		}
		await writeRepoFile(owner, repo, path, branch, next, current.sha, message, token);
	}

	await writeIfChanged(
		manifestPath,
		`${JSON.stringify(manifest, null, 2)}\n`,
		manifestFile,
		`chore: pin release version to ${requestedVersion}`,
	);
	await writeIfChanged(
		cargoTomlPath,
		cargoToml,
		cargoFile,
		`chore: pin workspace version to ${requestedVersion}`,
	);
	await writeIfChanged(
		tauriConfPath,
		`${JSON.stringify(tauriConf, null, 2)}\n`,
		tauriFile,
		`chore: pin tauri version to ${requestedVersion}`,
	);
	await writeIfChanged(
		packageJsonPath,
		`${JSON.stringify(packageJson, null, 2)}\n`,
		packageFile,
		`chore: pin ui package version to ${requestedVersion}`,
	);
	await writeIfChanged(
		changelogPath,
		changelog,
		changelogFile,
		`docs: pin changelog to ${requestedVersion}`,
	);

	const title = `chore(main): release ${requestedVersion}`;
	const body =
		typeof pr.body === "string" && pr.body.length > 0
			? pr.body.replaceAll(detectedVersion, requestedVersion)
			: pr.body;
	await github(`/repos/${owner}/${repo}/pulls/${prNumber}`, token, {
		method: "PATCH",
		body: JSON.stringify({ body, title }),
	});

	console.log(
		`Pinned release PR #${prNumber} from ${detectedVersion} to requested version ${requestedVersion}.`,
	);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
	main().catch((error) => {
		console.error(error);
		process.exit(1);
	});
}
