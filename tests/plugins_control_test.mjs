import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-plugins-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

const calls = [];
const manager = {
	listPlugins: async () => [
		{ entryId: "entry-1", moduleName: "@scope/tool", meta: { title: "Tool" }, enabled: true, fiberPhase: "active" },
	],
	listBundles: async () => [
		{ name: "@scope/bundle", meta: { title: "Bundle" }, enabled: false, available: true, installed: true },
	],
	setPluginEnabled: async (id, enabled) => { calls.push(["plugin", id, enabled]); return { stage: "enable", enabled }; },
	setBundleEnabled: async (name, enabled) => { calls.push(["bundle", name, enabled]); return { stage: "enable", enabled, restartReason: "no-hmr" }; },
};
let cleanup = async () => {};
const ctx = {
	agents: { list: () => [], get: () => undefined },
	systemPrompt: { context: () => () => {} },
	effect: fn => { cleanup = async () => { const dispose = fn(); if (typeof dispose === "function") await dispose(); }; },
	get: key => key === "pluginManager" ? manager : undefined,
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
	let result = await request("plugins.list", {});
	assert.match(result.text, /plugins .*booth/);
	assert.match(result.text, /enabled Tool \(@scope\/tool\)/);
	assert.match(result.text, /disabled @scope\/bundle · installed/);
	result = await request("plugins.set", { name: "@scope/tool", enabled: false });
	assert.match(result.text, /plugin @scope\/tool disabled/);
	assert.deepEqual(calls.at(-1), ["plugin", "entry-1", false]);
	result = await request("plugins.set", { name: "Bundle", enabled: true });
	assert.match(result.text, /bundle @scope\/bundle enabled/);
	assert.match(result.text, /restart required \(no-hmr\)/);
	assert.deepEqual(calls.at(-1), ["bundle", "@scope/bundle", true]);
	await assert.rejects(() => request("plugins.set", { name: "missing", enabled: true }), /no plugin or bundle/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
console.log("plugins control ok");
