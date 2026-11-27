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
		describe: () => [{ ns: "llm-pi-ai", value, user: value, revision }],
		mutate: async (ns, ops, expected) => {
			assert.equal(ns, "llm-pi-ai");
			assert.equal(expected, revision);
			for (const op of ops) {
				if (op.path[0] !== "providers" || op.path[1] !== "mock") continue;
				if (op.path.length === 2) {
					if (op.op === "unset") delete value.providers.mock;
					continue;
				}
				const field = op.path[2];
				if (op.op === "unset") delete value.providers.mock[field];
				else value.providers.mock[field] = op.value;
			}
			revision += 1;
		},
	},
	credentials: {
		describe: async ref => ({ configured: key !== null, writable: true, masked: key ? "sk-…" : "" }),
		set: async (ref, next) => { key = next; },
		unset: async () => { key = null; },
	},
	llm: {
		listConfigurableProviders: () => [
			{ provider: "mock", displayName: "Mock", settingsNs: "llm-pi-ai", settingsPath: ["providers", "mock"], declared: true, defaultBaseURL: "https://default.example" },
			{ provider: "other", displayName: "Other", settingsNs: "llm-pi-ai", settingsPath: ["providers", "other"], declared: false, defaultBaseURL: "https://other.example" },
		],
	},
	systemPrompt: { context: () => () => {} },
	on: () => () => {},
	effect: fn => { cleanup = async () => { const dispose = fn(); if (typeof dispose === "function") await dispose(); }; },
	get: keyName => keyName === "settings" ? ctx.settings : keyName === "credentials" ? ctx.credentials : keyName === "llm" ? ctx.llm : undefined,
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
	let listed = await request("providers.list", {});
	assert.equal(listed.providers.length, 2);
	assert.equal(listed.providers.find(row => row.provider === "other").configured, false);
	let result = await request("providers.get", { provider: "mock" });
	assert.equal(result.baseURL, "https://old.example/v1");
	assert.equal(result.apiKeyConfigured, false);
	result = await request("providers.get", { provider: "other" });
	assert.equal(result.configured, false);
	assert.equal(result.displayName, "Other");
	result = await request("providers.update", {
		provider: "mock", baseURL: "https://new.example/v1", api: "openai-completions",
		displayName: "Mock Gateway", models: ["one", "two"], apiKey: "sk-test-123",
	});
	assert.equal(result.baseURL, "https://new.example/v1");
	assert.equal(result.apiKeyConfigured, true);
	assert.equal(key, "sk-test-123");
	assert.equal(value.providers.mock.apiKeyEnv, "MOCK_API_KEY");
	assert.equal(value.providers.mock.api, "openai-completions");
	assert.equal(value.providers.mock.displayName, "Mock Gateway");
	assert.deepEqual(value.providers.mock.models, [{ id: "one" }, { id: "two" }]);
	result = await request("providers.remove", { provider: "mock", removeKey: true });
	assert.match(result.text, /Removed user configuration for mock and key MOCK_API_KEY/);
	assert.equal(value.providers.mock, undefined);
	assert.equal(key, null);
} finally {
	await cleanup();
	rmSync(root, { recursive: true, force: true });
}
