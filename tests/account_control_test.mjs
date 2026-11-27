import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-account-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

let cleanup = async () => {};
let signedIn = false;
let signInCalls = 0;
const account = {
	getState: async () => ({
		status: signedIn ? "credential-stored" : "signed-out",
		links: { usageUrl: "", topUpUrl: "" },
		attempt: signInCalls === 0 ? null : { id: "a-1", phase: "waiting-browser", authorizeUrl: "https://example.test/auth?a=1" },
	}),
	startSignIn: async () => { signInCalls += 1; return account.getState(); },
	signOut: async () => { signedIn = false; },
};
const ctx = {
	agents: { list: () => [], get: () => undefined },
	systemPrompt: { context: () => () => {} },
	effect: fn => { cleanup = async () => { const dispose = fn(); if (typeof dispose === "function") await dispose(); }; },
	get: key => key === "deepseekAccount" ? account : key === "webServer" ? { getOrigin: () => "http://127.0.0.1:1234" } : undefined,
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
	let result = await request("login", { sessionId: "s-1", args: "github" });
	assert.match(result.text, /provider "github" is unavailable/);
	result = await request("login", { sessionId: "s-1", args: "https://example.test/callback?code=1" });
	assert.match(result.text, /Manual OAuth redirect URLs/);
	result = await request("logout", { sessionId: "s-1", args: "github" });
	assert.match(result.text, /provider "github" is unavailable/);
	const login = await request("login", { sessionId: "s-1" });
	assert.equal(login.url, "https://example.test/auth?a=1");
	assert.match(login.text, /Open this URL/);
	signedIn = true;
	const logout = await request("logout", { sessionId: "s-1" });
	assert.match(logout.text, /Signed out/);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
