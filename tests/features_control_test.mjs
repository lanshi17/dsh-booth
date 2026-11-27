import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-features-"));
const socket = join(root, "control.sock");
const profile = join(root, "dsh", "profiles", "booth");
mkdirSync(profile, { recursive: true });
writeFileSync(join(profile, "package.json"), JSON.stringify({
	name: "probe",
	dependencies: {},
	dsh: { profile: { bundles: ["@deepseek-ai/dsh-base"] } },
}));
const fakeNpm = join(root, "fake-npm");
writeFileSync(fakeNpm, `#!/usr/bin/env node
const { readFileSync, writeFileSync } = require("node:fs");
const args = process.argv.slice(2);
const prefix = args[args.indexOf("--prefix") + 1];
const specs = args.slice(args.indexOf("--no-fund") + 1);
const path = prefix + "/package.json";
const document = JSON.parse(readFileSync(path, "utf8"));
document.dependencies ??= {};
for (const spec of specs) {
  const at = spec.lastIndexOf("@");
  document.dependencies[at > 0 ? spec.slice(0, at) : spec] = "0.2.1-rc.3";
}
writeFileSync(path, JSON.stringify(document, null, 2) + "\\n");
`, { mode: 0o755 });
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;
process.env.DSH_HOME = join(root, "dsh");
process.env.BOOTH_BASE_PROFILE = "booth";
process.env.BOOTH_NPM_BIN = fakeNpm;

let cleanup = async () => {};
const ctx = {
	agents: { list: () => [], get: () => undefined },
	systemPrompt: { context: () => () => {} },
	effect: fn => { cleanup = async () => { const dispose = fn(); if (typeof dispose === "function") await dispose(); }; },
	get: () => undefined,
	logger: { info: () => {}, warn: message => console.error(message) },
};
apply(ctx, { socket });

const waitFor = async predicate => {
	for (let i = 0; i < 100; i += 1) {
		if (predicate()) return;
		await new Promise(resolve => setTimeout(resolve, 20));
	}
	throw new Error("control socket did not start");
};
const request = (method, params) => new Promise((resolve, reject) => {
	const client = createConnection(socket);
	let buffer = "";
	client.on("error", reject);
	client.on("data", chunk => {
		buffer += chunk.toString("utf8");
		const end = buffer.indexOf("\n");
		if (end < 0) return;
		const reply = JSON.parse(buffer.slice(0, end));
		client.end();
		if (reply.error) reject(new Error(reply.error.message));
		else resolve(reply.result);
	});
	client.on("connect", () => client.write(`${JSON.stringify({ id: 1, method, params })}\n`));
});

try {
	await waitFor(() => existsSync(socket));
	let result = await request("browser", { args: "install" });
	assert.match(result.text, /installed and enabled/);
	let features = JSON.parse(readFileSync(join(root, "features.json"), "utf8"));
	assert.equal(features.browser.enabled, true);
	const installed = JSON.parse(readFileSync(join(profile, "package.json"), "utf8"));
	assert.ok(installed.dependencies["@deepseek-ai/dsh-browser-use"]);
	result = await request("browser", { args: "headless" });
	assert.match(result.text, /headless/);
	features = JSON.parse(readFileSync(join(root, "features.json"), "utf8"));
	assert.equal(features.browser.headless, true);
	result = await request("browser", { args: "off" });
	assert.match(result.text, /disabled/);
	features = JSON.parse(readFileSync(join(root, "features.json"), "utf8"));
	assert.equal(features.browser.enabled, false);
	result = await request("computer", { args: "status" });
	assert.match(result.text, /computer: off/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
