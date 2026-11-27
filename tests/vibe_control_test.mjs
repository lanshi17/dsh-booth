import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-vibe-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

let prompt = "";
const agent = { id: "s-1", status: "idle", session: {} };
let cleanup = async () => {};
const ctx = {
	agents: { get: id => id === "s-1" ? agent : undefined, list: () => [agent] },
	subagents: {
		startActivation: async request => {
			prompt = request.request.prompt[0].text;
			return {
				childId: "vibe-child",
				result: Promise.resolve({ output: [{ type: "text", text: "read-only report" }], stopReason: "end_turn" }),
				dispose: async () => {},
			};
		},
	},
	systemPrompt: { context: () => () => {} },
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
	let result = await request("vibe", { sessionId: "s-1", args: "inspect the parser" });
	assert.match(result.text, /read-only report/);
	assert.match(prompt, /inspect the parser/);
	assert.match(prompt, /Do not edit files/);
	result = await request("vibe", { sessionId: "s-1", args: "on" });
	assert.match(result.text, /Vibe mode on/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
