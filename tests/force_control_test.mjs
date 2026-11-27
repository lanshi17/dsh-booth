import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-force-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

const hooks = new Map();
let restricted = null;
let disposed = false;
const agent = {
	id: "s-1",
	status: "idle",
	session: {},
	ctx: {
		tools: {
			restrict: filter => {
				restricted = filter;
				return () => { disposed = true; };
			},
		},
	},
};
let cleanup = async () => {};
const ctx = {
	agents: { get: id => id === "s-1" ? agent : undefined, list: () => [agent] },
	tools: { definitions: () => [{ name: "read" }, { name: "write" }] },
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
	let result = await request("force", { sessionId: "s-1", args: "read" });
	assert.match(result.text, /Force armed/);
	for (const fn of hooks.get("agent/inbox/inserted") ?? []) fn({ agent, message: { source: { kind: "user" } } });
	assert.deepEqual(restricted, { allow: ["read"] });
	for (const fn of hooks.get("agent/turn-stopping") ?? []) fn({ agent });
	assert.equal(disposed, true);
	result = await request("force", { sessionId: "s-1", args: "nope" });
	assert.match(result.text, /not available/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
