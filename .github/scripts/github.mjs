export const semverPattern = /^[0-9]+\.[0-9]+\.[0-9]+$/;

export function requireEnv(value, name) {
	if (!value) {
		throw new Error(`${name} must be set.`);
	}
	return value;
}

export function assertExactSemver(value, name = "version") {
	if (!semverPattern.test(value ?? "")) {
		throw new Error(`${name} must be exact semver without a v prefix: ${value}.`);
	}
}

export function compareSemver(left, right) {
	const leftParts = left.split(".").map(Number);
	const rightParts = right.split(".").map(Number);
	for (let index = 0; index < 3; index += 1) {
		if (leftParts[index] !== rightParts[index]) {
			return leftParts[index] - rightParts[index];
		}
	}
	return 0;
}

export function decodeBase64(value) {
	return Buffer.from(value, "base64").toString("utf8");
}

export function encodeBase64(value) {
	return Buffer.from(value, "utf8").toString("base64");
}

export function parseReleasePleasePrs(raw) {
	if (!raw) {
		return [];
	}

	try {
		const parsed = JSON.parse(raw);
		if (Array.isArray(parsed)) {
			return parsed;
		}
		if (parsed && typeof parsed === "object") {
			return Object.values(parsed).filter((value) => value && typeof value === "object");
		}
	} catch (error) {
		const message = error instanceof Error ? error.message : String(error);
		console.log(`Could not parse release-please PR output: ${message}`);
	}

	return [];
}

export function releasePrNumber(prs) {
	if (prs.length === 0) {
		throw new Error("No release PR was returned by Release Please.");
	}
	const number = prs[0]?.number ?? prs[0]?.pullRequestNumber;
	if (!number) {
		throw new Error("No release PR number was returned by Release Please.");
	}
	return Number(number);
}

export async function github(path, token, options = {}) {
	const response = await fetch(`https://api.github.com${path}`, {
		...options,
		headers: {
			Accept: "application/vnd.github+json",
			Authorization: `Bearer ${token}`,
			"Content-Type": "application/json",
			"X-GitHub-Api-Version": "2022-11-28",
			...(options.headers ?? {}),
		},
	});

	if (!response.ok) {
		throw new Error(`GitHub API ${path} failed: ${response.status} ${await response.text()}`);
	}

	return response.status === 204 ? null : response.json();
}

export async function readRepoFile(owner, repo, path, ref, token) {
	const file = await github(
		`/repos/${owner}/${repo}/contents/${path}?ref=${encodeURIComponent(ref)}`,
		token,
	);
	return {
		content: decodeBase64(file.content),
		sha: file.sha,
	};
}

export async function writeRepoFile(owner, repo, path, branch, content, sha, message, token) {
	await github(`/repos/${owner}/${repo}/contents/${path}`, token, {
		method: "PUT",
		body: JSON.stringify({
			branch,
			content: encodeBase64(content),
			message,
			sha,
		}),
	});
}
