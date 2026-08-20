import assert from "node:assert/strict";
import test from "node:test";
import { assertDraftReleaseAssets, canStartPublish, requiredReleaseAssetNames } from "./github.mjs";

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

const completeAssets = [
	{ name: "Crosspond_0.1.0_aarch64.dmg", size: 42 },
	{ name: "Crosspond.app.tar.gz", size: 10 },
	{ name: "Crosspond.app.tar.gz.sig", size: 1 },
	{ name: "latest.json", size: 2 },
];

test("assertDraftReleaseAssets accepts a draft whose DMG size matches", () => {
	assert.deepEqual(
		assertDraftReleaseAssets({
			isDraft: true,
			assets: completeAssets,
			localDmgName: "Crosspond_0.1.0_aarch64.dmg",
			localDmgSize: 42,
		}),
		completeAssets.map((asset) => asset.name),
	);
});

test("assertDraftReleaseAssets refuses a public release", () => {
	assert.throws(
		() =>
			assertDraftReleaseAssets({
				isDraft: false,
				assets: completeAssets,
				localDmgName: "Crosspond_0.1.0_aarch64.dmg",
				localDmgSize: 42,
			}),
		/must still be a draft/,
	);
});

test("assertDraftReleaseAssets refuses a size mismatch that would leave a pre-staple DMG", () => {
	assert.throws(
		() =>
			assertDraftReleaseAssets({
				isDraft: true,
				assets: completeAssets,
				localDmgName: "Crosspond_0.1.0_aarch64.dmg",
				localDmgSize: 99,
			}),
		/pre-staple/,
	);
});
