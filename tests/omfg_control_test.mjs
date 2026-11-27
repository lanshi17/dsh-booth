import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-omfg-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

const agent = { id: "s-1", status: "idle", session: {} };
const contexts = [];
let prompt = "";
let cleanup = async () => {};
const ctx = {
	agents: { get: id => id === "s-1" ? agent : undefined, list: () => [agent] },
	systemPrompt: { context: value => { contexts.push(value); return () => {}; } },
	subagents: {
		startActivation: async request => {
			prompt = request.request.prompt[0].text;
			return {
				childId: "child-rule",
				result: Promise.resolve({ output: [{ type: "text", text: "Never repeat the same command twice." }], stopReason: "end_turn" }),
				dispose: async () => {},
			};
		},
	},
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
	const result = await request("omfg", { sessionId: "s-1", complaint: "You repeat the same command" });
	assert.match(result.text, /Rule added: Never repeat/);
	assert.match(prompt, /You repeat the same command/);
	const rules = readFileSync(join(root, "rules.md"), "utf8");
	assert.match(rules, /Never repeat the same command twice/);
	const ruleContext = contexts.find(value => value.name === "booth-rules");
	assert.ok(ruleContext);
	assert.match(ruleContext.text(), /<booth-rules>/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
