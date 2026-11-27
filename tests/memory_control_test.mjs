import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-memory-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

const contexts = [];
let cleanup = async () => {};
const ctx = {
	agents: { list: () => [] },
	systemPrompt: { context: value => { contexts.push(value); return () => {}; } },
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
	const memoryContext = contexts.find(value => value.name === "booth-memory");
	assert.ok(memoryContext);
	assert.equal(memoryContext.text(), "");
	assert.equal((await request("memory", { args: "add keep tests green" })).text, "Memory note added.");
	const note = readFileSync(join(root, "memory.md"), "utf8");
	assert.match(note, /keep tests green/);
	assert.match(memoryContext.text(), /<booth-memory>/);
	const stats = await request("memory", { args: "stats" });
	assert.match(stats.text, /notes: 1/);
	const sync = await request("memory", { args: "sync" });
	assert.match(sync.text, /Notes: 1/);
	assert.equal((await request("memory", { args: "clear" })).text, "Memory cleared.");
	assert.equal(memoryContext.text(), "");
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
