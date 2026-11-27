import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-market-"));
const socket = join(root, "control.sock");
const profile = join(root, "dsh", "profiles", "booth");
mkdirSync(profile, { recursive: true });
writeFileSync(join(profile, "package.json"), JSON.stringify({
	name: "probe",
	dependencies: {},
	dsh: { profile: { bundles: ["@deepseek-ai/dsh-base"] } },
}));
const catalog = join(root, "catalog.json");
writeFileSync(catalog, JSON.stringify({
	plugins: [{ name: "sample", description: "Sample plugin", package: "@scope/sample", version: "1.2.3", bundle: "@scope/sample" }],
}));
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;
process.env.DSH_HOME = join(root, "dsh");
process.env.BOOTH_BASE_PROFILE = "booth";
const fakeNpm = join(root, "fake-npm");
writeFileSync(fakeNpm, `#!/usr/bin/env node
const { readFileSync, writeFileSync } = require("node:fs");
const args = process.argv.slice(2);
const prefix = args[args.indexOf("--prefix") + 1];
const verb = args[0];
const spec = args[args.length - 1];
const path = prefix + "/package.json";
const document = JSON.parse(readFileSync(path, "utf8"));
document.dependencies ??= {};
if (verb === "install") {
  const at = spec.lastIndexOf("@");
  const name = at > 0 ? spec.slice(0, at) : spec;
  document.dependencies[name] = "1.2.3";
}
if (verb === "uninstall") delete document.dependencies[spec];
writeFileSync(path, JSON.stringify(document, null, 2) + "\\n");
`, { mode: 0o755 });
process.env.BOOTH_NPM_BIN = fakeNpm;

let cleanup = async () => {};
const ctx = {
	agents: { list: () => [] },
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
	let result = await request("marketplace", { args: `add ${catalog} --name local` });
	assert.match(result.text, /Added marketplace "local" \(1 plugins\)/);
	result = await request("marketplace", { args: "discover" });
	assert.match(result.text, /sample \| 1\.2\.3 \| Sample plugin \[local\]/);
	result = await request("marketplace", { args: "list" });
	assert.match(result.text, /local/);
	result = await request("marketplace", { args: "installed" });
	assert.match(result.text, /No plugins installed/);
	result = await request("marketplace", { args: "install sample@local" });
	assert.match(result.text, /Installed @scope\/sample@1\.2\.3/);
	result = await request("marketplace", { args: "installed" });
	assert.match(result.text, /@scope\/sample 1\.2\.3/);
	const profilePackage = JSON.parse(readFileSync(join(profile, "package.json"), "utf8"));
	assert.ok(profilePackage.dsh.profile.bundles.includes("@scope/sample"));
	result = await request("marketplace", { args: "uninstall sample@local" });
	assert.match(result.text, /Uninstalled/);
	result = await request("marketplace", { args: "installed" });
	assert.match(result.text, /No plugins installed/);
	result = await request("marketplace", { args: "update local" });
	assert.match(result.text, /1 plugins/);
	result = await request("marketplace", { args: "remove local" });
	assert.match(result.text, /Removed marketplace/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
