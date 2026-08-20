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
