import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-advisor-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

const agent = { id: "s-1", status: "idle", session: {} };
const hooks = new Map();
let prompt = "";
let cleanup = async () => {};
const ctx = {
	agents: { get: id => id === "s-1" ? agent : undefined, list: () => [agent] },
	sessionQuery: {
		observeSession: async () => ({
			events: [
				{ type: "user/message", data: { content: [{ type: "text", text: "fix the parser" }] } },
				{ type: "assistant/message", data: { content: [{ type: "text", text: "I changed token handling." }] } },
			],
			[Symbol.dispose]: () => {},
		}),
	},
	subagents: {
		startActivation: async request => {
			prompt = request.request.prompt[0].text;
			return {
				childId: "child-advisor",
				result: Promise.resolve({ output: [{ type: "text", text: "- Add a regression test for empty input." }], stopReason: "end_turn" }),
				dispose: async () => {},
			};
		},
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
	throw new Error("condition did not settle");
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
	let result = await request("advisor", { sessionId: "s-1", args: "on" });
	assert.match(result.text, /Advisor enabled/);
	assert.ok(hooks.has("agent/turn-stopping"));
	for (const fn of hooks.get("agent/turn-stopping")) fn({ agent });
	await waitFor(() => prompt.length > 0);
	assert.match(prompt, /fix the parser/);
	let notes = "";
	for (let i = 0; i < 50; i += 1) {
		result = await request("advisor", { sessionId: "s-1", args: "dump" });
		notes = result.text;
		if (/regression test/.test(notes)) break;
		await new Promise(resolve => setTimeout(resolve, 20));
	}
	assert.match(notes, /Add a regression test/);
	result = await request("advisor", { sessionId: "s-1", args: "status" });
	assert.match(result.text, /Advisor: on/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
