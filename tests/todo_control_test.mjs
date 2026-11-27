import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-todo-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

const appended = [];
const agent = {
	id: "s-1",
	status: "idle",
	busy: false,
	session: { header: { cwd: "/work" }, append: (type, data) => appended.push({ type, data }) },
};
let cleanup = async () => {};
const ctx = {
	agents: { get: id => id === "s-1" ? agent : undefined, list: () => [agent] },
	sessionQuery: {
		listSessions: async () => [
			{ header: { id: "s-live", parentSession: "s-1", cwd: "/work" } },
			{ header: { id: "s-done", parentSession: "s-1", cwd: "/work" } },
		],
	},
	systemPrompt: { context: () => () => {} },
	effect: fn => { cleanup = async () => { const dispose = fn(); if (typeof dispose === "function") await dispose(); }; },
	get: () => undefined,
	emit: () => {},
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
	client.write(`${JSON.stringify({ method, params })}\n`);
});

await waitFor(() => true);
const committed = await request("todos.commit", {
	sessionId: "s-1",
	todos: [
		{ content: "task one", status: "pending" },
		{ content: "task two", status: "in_progress" },
		{ content: "task three", status: "completed" },
	],
});
assert.equal(committed.count, 3);
assert.equal(appended.length, 1);
assert.equal(appended[0].type, "todo/write");
assert.deepEqual(appended[0].data.todos, [
	{ content: "task one", status: "pending" },
	{ content: "task two", status: "in_progress" },
	{ content: "task three", status: "completed" },
]);
await assert.rejects(() => request("todos.commit", {
	sessionId: "s-1",
	todos: [{ content: "bad", status: "unknown" }],
}));

const rows = await request("agents.list", { sessionId: "s-1" });
assert.equal(rows.length, 3);
const finished = rows.find(row => row.sessionId === "s-done");
assert.equal(finished.status, "finished");
assert.equal(finished.finished, true);
assert.equal(rows.find(row => row.sessionId === "s-1").finished, false);

await cleanup();
rmSync(root, { recursive: true, force: true });
console.log("todo control ok");
