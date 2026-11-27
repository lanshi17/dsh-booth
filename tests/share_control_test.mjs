import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-share-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

const agent = { id: "s-1", status: "idle", session: {} };
let cleanup = async () => {};
const ctx = {
	agents: { get: id => id === "s-1" ? agent : undefined, list: () => [agent] },
	sessionQuery: {
		observeSession: async () => ({
			events: [
				{ type: "user/message", time: 1, data: { content: [{ type: "text", text: "hello" }] } },
				{ type: "assistant/message", time: 2, data: { content: [{ type: "text", text: "hi there" }] } },
			],
			[Symbol.dispose]: () => {},
		}),
	},
	sessionTitle: { get: () => ({ title: "Shared probe" }) },
	systemPrompt: { context: () => () => {} },
	on: () => () => {},
	effect: fn => { cleanup = async () => { const dispose = fn(); if (typeof dispose === "function") await dispose(); }; },
	get: key => key === "sessionTitle" ? ctx.sessionTitle : undefined,
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
	let result = await request("share", { sessionId: "s-1", args: "" });
	assert.match(result.url, /^http:\/\/127\.0\.0\.1:\d+\/s\/[a-f0-9]+$/);
	const shareUrl = result.url;
	const page = await fetch(shareUrl);
	assert.equal(page.status, 200);
	assert.match(await page.text(), /booth share/);
	const token = shareUrl.split("/s/")[1];
	let snapshot = await fetch(`${new URL(shareUrl).origin}/api/snapshot?token=${token}`);
	assert.equal(snapshot.status, 200);
	let body = await snapshot.json();
	assert.equal(body.write, false);
	assert.match(body.entries[1].text, /hi there/);
	result = await request("collab", { sessionId: "s-1", args: "on" });
	assert.match(result.text, /control/);
	snapshot = await fetch(`${new URL(shareUrl).origin}/api/snapshot?token=${token}`);
	body = await snapshot.json();
	assert.equal(body.write, true);
	result = await request("collab", { sessionId: "s-1", args: "view" });
	assert.match(result.text, /view-only/);
	result = await request("collab", { sessionId: "s-1", args: "list" });
	assert.match(result.text, /Shared probe/);
	result = await request("collab", { sessionId: "s-1", args: "status" });
	assert.match(result.text, /view-only/);
	result = await request("collab", { sessionId: "s-1", args: "stop" });
	assert.match(result.text, /stopped/i);
	result = await request("leave", { sessionId: "s-1" });
	assert.match(result.text, /No active share/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
