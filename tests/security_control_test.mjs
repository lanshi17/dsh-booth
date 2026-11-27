import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-security-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

const agent = { id: "s-1", busy: false, status: "idle", session: {} };
let prompt = "";
let cleanup = async () => {};
const ctx = {
	agents: { get: id => id === "s-1" ? agent : undefined, list: () => [agent] },
	systemPrompt: { context: () => () => {} },
	subagents: {
		startActivation: async request => {
			prompt = request.request.prompt[0].text;
			return {
				childId: "child-sec",
				result: Promise.resolve({ output: [{ type: "text", text: "Summary: one issue\nFindings: token in config" }], stopReason: "end_turn" }),
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
	const started = await request("security", { sessionId: "s-1", args: "scan auth --all" });
	assert.match(started.text, /Security scan started: (scan-[^ ]+)/);
	const id = started.id;
	assert.match(prompt, /auth/);
	assert.match(prompt, /entire repository/);
	await waitFor(() => true);
	let status = await request("security", { sessionId: "s-1", args: `status ${id}` });
	for (let i = 0; i < 50 && !/complete/.test(status.text); i += 1) {
		await new Promise(resolve => setTimeout(resolve, 20));
		status = await request("security", { sessionId: "s-1", args: `status ${id}` });
	}
	assert.match(status.text, /complete/);
	const show = await request("security", { sessionId: "s-1", args: `show ${id}` });
	assert.match(show.text, /Findings: token in config/);
	const scans = await request("security", { sessionId: "s-1", args: "scans" });
	assert.match(scans.text, new RegExp(id));
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
