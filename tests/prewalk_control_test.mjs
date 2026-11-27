import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-prewalk-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

let selected = { provider: "mock", model: "model" };
const agent = {
	id: "s-1",
	status: "idle",
	model: {
		current: () => selected,
		select: value => { selected = value; },
	},
};
const hooks = new Map();
let cleanup = async () => {};
const ctx = {
	agents: { get: id => id === "s-1" ? agent : undefined, list: () => [agent] },
	llm: {
		listProviders: () => [{ id: "mock", name: "Mock" }],
		listModels: async () => [{ provider: "mock", id: "model", name: "Model" }, { provider: "mock", id: "other", name: "Other" }],
	},
	systemPrompt: { context: () => () => {} },
	on: (event, fn) => { const list = hooks.get(event) ?? []; list.push(fn); hooks.set(event, list); return () => {}; },
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
	let result = await request("prewalk", { sessionId: "s-1", args: "other" });
	assert.match(result.text, /Prewalk on: switching to mock\/other/);
	assert.ok(hooks.has("agent/inbox/inserted"));
	assert.ok(hooks.has("agent/turn-stopping"));
	for (const fn of hooks.get("agent/inbox/inserted")) fn({ agent, message: { source: { kind: "user" } } });
	assert.deepEqual(selected, { provider: "mock", model: "other" });
	result = await request("prewalk", { sessionId: "s-1", args: "status" });
	assert.match(result.text, /Prewalk active/);
	for (const fn of hooks.get("agent/turn-stopping")) fn({ agent });
	assert.deepEqual(selected, { provider: "mock", model: "model" });
	result = await request("prewalk", { sessionId: "s-1", args: "status" });
	assert.match(result.text, /Prewalk off/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
