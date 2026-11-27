import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-skillful-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

const contexts = [];
let denied = null;
let restored = false;
const agent = {
	id: "s-1",
	status: "idle",
	session: {},
	ctx: { tools: { restrict: filter => { denied = filter; return () => { restored = true; }; } } },
};
let cleanup = async () => {};
const ctx = {
	agents: { get: id => id === "s-1" ? agent : undefined, list: () => [agent] },
	tools: { definitions: () => [{ name: "skill" }, { name: "read" }] },
	systemPrompt: { context: value => { contexts.push(value); return () => {}; } },
	on: () => () => {},
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
	const context = contexts.find(value => value.name === "booth-skills-off");
	assert.ok(context);
	assert.equal(context.text({ session: { id: "s-1" } }), "");
	let result = await request("skillful", { sessionId: "s-1", args: "off" });
	assert.match(result.text, /disabled/);
	assert.deepEqual(denied, { deny: ["skill"] });
	assert.match(context.text({ session: { id: "s-1" } }), /booth-skills-off/);
	result = await request("skillful", { sessionId: "s-1", args: "status" });
	assert.match(result.text, /Skill listing: off/);
	result = await request("skillful", { sessionId: "s-1", args: "on" });
	assert.match(result.text, /enabled/);
	assert.equal(restored, true);
	assert.equal(context.text({ session: { id: "s-1" } }), "");
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
