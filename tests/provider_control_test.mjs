import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { apply } from "../plugins/booth-control.mjs";

const root = mkdtempSync(join(tmpdir(), "booth-provider-"));
const socket = join(root, "control.sock");
process.env.BOOTH_STATE_DIR = root;
process.env.BOOTH_CONTROL_SOCKET = socket;

let value = { providers: { mock: { baseURL: "https://old.example/v1" } } };
let revision = 3;
let key = null;
let cleanup = async () => {};
const ctx = {
	agents: { list: () => [], get: () => undefined },
	settings: {
		describe: () => [{ ns: "llm-pi-ai", value, revision }],
		mutate: async (ns, ops, expected) => {
			assert.equal(ns, "llm-pi-ai");
			assert.equal(expected, revision);
			for (const op of ops) {
				if (op.path.at(-1) === "baseURL") value.providers.mock.baseURL = op.value;
				if (op.path.at(-1) === "apiKeyEnv") value.providers.mock.apiKeyEnv = op.value;
			}
			revision += 1;
		},
	},
	credentials: {
		describe: async ref => ({ configured: key !== null, writable: true, masked: key ? "sk-…" : "" }),
		set: async (ref, next) => { key = next; },
	},
	systemPrompt: { context: () => () => {} },
	on: () => () => {},
	effect: fn => { cleanup = async () => { const dispose = fn(); if (typeof dispose === "function") await dispose(); }; },
	get: keyName => keyName === "settings" ? ctx.settings : keyName === "credentials" ? ctx.credentials : undefined,
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
	let result = await request("providers.get", { provider: "mock" });
	assert.equal(result.baseURL, "https://old.example/v1");
	assert.equal(result.apiKeyConfigured, false);
	result = await request("providers.update", { provider: "mock", baseURL: "https://new.example/v1", apiKey: "sk-test-123" });
	assert.equal(result.baseURL, "https://new.example/v1");
	assert.equal(result.apiKeyConfigured, true);
	assert.equal(key, "sk-test-123");
	assert.equal(value.providers.mock.apiKeyEnv, "MOCK_API_KEY");
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
