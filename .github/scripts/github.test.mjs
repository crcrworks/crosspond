import assert from "node:assert/strict";
import test from "node:test";
import { canStartPublish, requiredReleaseAssetNames } from "./github.mjs";

test("allows a first publish when nothing exists", () => {
	assert.doesNotThrow(() =>
		canStartPublish({
			requestedVersion: "0.1.0",
			existingRelease: null,
			publishedVersions: [],
		}),
	);
});

test("allows retrying a leftover draft", () => {
	assert.doesNotThrow(() =>
		canStartPublish({
			requestedVersion: "0.1.0",
			existingRelease: { draft: true },
			publishedVersions: [],
		}),
	);
});

test("refuses a public release of the same version", () => {
	assert.throws(
		() =>
			canStartPublish({
				requestedVersion: "0.1.0",
				existingRelease: { draft: false },
				publishedVersions: ["0.1.0"],
			}),
		/already published/,
	);
});

test("refuses a version that is not newer than a public release", () => {
	assert.throws(
		() =>
			canStartPublish({
				requestedVersion: "0.1.0",
				existingRelease: null,
				publishedVersions: ["0.2.0"],
			}),
		/not newer/,
	);
});

test("requiredReleaseAssetNames lists missing updater and dmg files", () => {
	assert.deepEqual(requiredReleaseAssetNames([]), [
		".dmg",
		".app.tar.gz",
		".app.tar.gz.sig",
		"latest.json",
	]);
	assert.deepEqual(
		requiredReleaseAssetNames([
			"Crosspond_0.1.0_aarch64.dmg",
			"Crosspond.app.tar.gz",
			"Crosspond.app.tar.gz.sig",
			"latest.json",
		]),
		[],
	);
});
