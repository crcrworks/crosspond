import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

test("tauri beforeBuildCommand cwd resolves to the repo scripts path", () => {
	const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
	const tauriConf = JSON.parse(
		readFileSync(resolve(repoRoot, "crates/crosspond-app/tauri.conf.json"), "utf8"),
	);
	const confDir = resolve(repoRoot, "crates/crosspond-app");
	const cwd = resolve(confDir, tauriConf.build.beforeBuildCommand.cwd);
	assert.equal(cwd, repoRoot);
	const scriptWord = tauriConf.build.beforeBuildCommand.script.split(" ")[1];
	assert.equal(scriptWord, "scripts/stage-chrome-host.sh");
	assert.ok(existsSync(resolve(cwd, scriptWord)));

	const devCwd = resolve(confDir, tauriConf.build.beforeDevCommand.cwd);
	assert.equal(devCwd, repoRoot);
	const devScript = tauriConf.build.beforeDevCommand.script.split(" ")[1];
	assert.equal(devScript, "scripts/stage-chrome-host.sh");
	assert.ok(existsSync(resolve(devCwd, devScript)));
});

test("publish-release notarizes and staples the DMG after tauri-action, then verifies", () => {
	const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
	const yml = readFileSync(resolve(repoRoot, ".github/workflows/publish-release.yml"), "utf8");
	const notarize = readFileSync(resolve(repoRoot, ".github/scripts/notarize-staple-dmg.sh"), "utf8");
	const verify = readFileSync(resolve(repoRoot, ".github/scripts/verify-macos-release.sh"), "utf8");

	const tauriAction = yml.indexOf(
		"uses: tauri-apps/tauri-action@84b9d35b5fc46c1e45415bdb6144030364f7ebc5 # v0",
	);
	const notarizeStep = yml.indexOf("notarize-staple-dmg.sh");
	const verifyStep = yml.indexOf("verify-macos-release.sh");
	assert.ok(tauriAction >= 0, "tauri-action must be pinned to the v0 commit SHA");
	assert.match(yml, /tauri-apps\/tauri-action@[0-9a-f]{40} # v0/);
	assert.ok(tauriAction < notarizeStep, "DMG notarization must run after tauri-action");
	assert.ok(notarizeStep < verifyStep, "final verify must run after DMG re-upload");
	assert.match(yml, /needs:\n      - bundle\n      - draft_release/);

	const submit = notarize.indexOf("notarytool submit");
	const staple = notarize.indexOf("stapler staple");
	const validate = notarize.indexOf("stapler validate");
	const clobber = notarize.indexOf("gh release upload");
	assert.ok(submit >= 0);
	assert.ok(submit < staple);
	assert.ok(staple < validate);
	assert.ok(validate < clobber);
	assert.match(notarize, /--key "\$APPLE_API_KEY_PATH"/);
	assert.match(notarize, /--apple-id "\$APPLE_ID"/);
	assert.match(notarize, /gh release upload "\$tag" "\$DMG" --clobber/);

	assert.match(verify, /codesign --verify --deep --strict --verbose=2/);
	assert.match(verify, /spctl --assess --type execute -vv/);
	assert.match(verify, /stapler validate/);
	assert.match(verify, /assertDraftReleaseAssets/);
});
