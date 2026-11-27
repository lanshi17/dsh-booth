/**
 * booth-control: the out-of-band control plane between booth and the harness.
 *
 * The ACP bridge stays a standard automation surface. This plugin lives in the
 * same dsh process, owns a private Unix socket, and executes the operations an
 * interactive front end needs but ACP does not expose: native slash commands,
 * session rename/fork/delete, goal and plan state, background jobs, and the
 * live agent roster. Requests and responses are newline-delimited JSON.
 */

import { chmodSync, existsSync, mkdirSync, readFileSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { execFile, execFileSync } from "node:child_process";
import { promisify } from "node:util";
import { createRequire } from "node:module";
import { createServer as createHttpServer } from "node:http";
import { createServer } from "node:net";
import { basename, dirname, join } from "node:path";
import { randomUUID } from "node:crypto";
import { fileURLToPath, pathToFileURL } from "node:url";

export const name = "booth-control";

/**
 * The ACP profile mounts every one of these. Declaring them lets Cordis expose
 * the services on this plugin's context; a narrower profile simply keeps the
 * control plugin pending instead of answering half its surface.
 */
export const inject = [
	"agents",
	"commands",
	"goals",
	"jobs",
	"llm",
	"planMode",
	"sandboxPolicy",
	"sessionPersistence",
	"sessionQuery",
	"sessions",
	"sessionTitle",
	"subagents",
	"systemPrompt",
	"settings",
	"credentials",
	"toolResultPruner",
	"tools",
];

const PROTOCOL_VERSION = 1;

/** Resolve a harness package from the running dsh installation, not from this file. */
function harnessRequire() {
	const entry = process.argv[1];
	if (!entry) throw new Error("booth-control: cannot locate the running dsh entry point");
	let real = entry;
	try {
		real = realpathSync(entry);
	} catch {
		// Keep the unresolved path; createRequire still anchors on its directory.
	}
	return createRequire(real);
}

/** Import one harness subpath once. */
const modules = new Map();
async function harnessModule(specifier) {
	if (!modules.has(specifier)) {
		const resolved = harnessRequire().resolve(specifier);
		modules.set(specifier, import(pathToFileURL(resolved).href));
	}
	return modules.get(specifier);
}

class ControlError extends Error {
	constructor(code, message) {
		super(message);
		this.code = code;
	}
}

function fail(code, message) {
	throw new ControlError(code, message);
}

function messageOf(error) {
	if (error instanceof Error) return error.message;
	return String(error);
}

function codeOf(error) {
	if (error && typeof error === "object" && typeof error.code === "string") return error.code;
	return "control/internal";
}

/** Latest boundary at which a fork may cut: the last completed turn plus its tail. */
function latestBoundary(events) {
	const lastTurnEnd = events.findLast(event => event.type === "turn/end");
	if (lastTurnEnd === undefined) return undefined;
	let boundary = lastTurnEnd.seq;
	for (const next of events.slice(boundary + 1)) {
		if (
			next.type === "turn/start" ||
			(next.type === "user/message" && next.surfaceOp === "append") ||
			next.type === "agent/inbox/spliced"
		) {
			break;
		}
		boundary = next.seq;
	}
	return boundary;
}

function sanitizeResult(result) {
	if (!result || typeof result !== "object") return { kind: "success", text: "" };
	return {
		kind: result.kind === "error" ? "error" : "success",
		text: typeof result.text === "string" ? result.text : "",
		...(typeof result.sourceEventSeq === "number" ? { sourceEventSeq: result.sourceEventSeq } : {}),
	};
}

/**
 * One process-wide control service. The plugin disposes the socket and every
 * in-flight command with the profile.
 */
export function apply(ctx, config) {
	const socketPath =
		config?.socket ??
		process.env.BOOTH_CONTROL_SOCKET ??
		join(process.env.DSH_HOME ?? join(process.env.HOME ?? ".", ".dsh"), "state", "booth", "control.sock");
	const active = new Set();
	const sessionDirs = new Map();
	let server;
	let closed = false;
	const stateDir =
		process.env.BOOTH_STATE_DIR ??
		join(process.env.DSH_HOME ?? join(process.env.HOME ?? ".", ".dsh"), "state", "booth");
	const memoryPath = join(stateDir, "memory.md");
	const rulesPath = join(stateDir, "rules.md");
	const readMemory = () => {
		try {
			return readFileSync(memoryPath, "utf8");
		} catch (error) {
			if (error?.code === "ENOENT") return "";
			throw error;
		}
	};
	const writeMemory = text => {
		mkdirSync(stateDir, { recursive: true, mode: 0o700 });
		const temp = `${memoryPath}.tmp`;
		writeFileSync(temp, text, { mode: 0o600 });
		renameSync(temp, memoryPath);
		chmodSync(memoryPath, 0o600);
		ctx.emit?.("system-prompt/change");
	};
	const memoryText = () => {
		const text = readMemory().trim();
		return text.length === 0 ? "" : `<booth-memory>\n${text}\n</booth-memory>`;
	};
	ctx.systemPrompt.context({
		name: "booth-memory",
		order: 240,
		interpolate: false,
		text: memoryText,
	});
	const readRules = () => {
		try {
			return readFileSync(rulesPath, "utf8");
		} catch (error) {
			if (error?.code === "ENOENT") return "";
			throw error;
		}
	};
	const writeRules = text => {
		mkdirSync(stateDir, { recursive: true, mode: 0o700 });
		const temp = `${rulesPath}.tmp`;
		writeFileSync(temp, text, { mode: 0o600 });
		renameSync(temp, rulesPath);
		chmodSync(rulesPath, 0o600);
		ctx.emit?.("system-prompt/change");
	};
	const rulesText = () => {
		const text = readRules().trim();
		return text.length === 0
			? ""
			: `<booth-rules>\nFollow these behavioral rules in every turn:\n${text}\n</booth-rules>`;
	};
	ctx.systemPrompt.context({
		name: "booth-rules",
		order: 241,
		interpolate: false,
		text: rulesText,
	});

	const marketplacesPath = join(stateDir, "marketplaces.json");
	const marketplaceCache = join(stateDir, "marketplace");
	const profileDir = join(
		process.env.DSH_HOME ?? join(process.env.HOME ?? ".", ".dsh"),
		"profiles",
		process.env.BOOTH_BASE_PROFILE ?? process.env.BOOTH_PROFILE ?? "booth",
	);
	const readJson = (path, fallback) => {
		try {
			return JSON.parse(readFileSync(path, "utf8"));
		} catch (error) {
			if (error?.code === "ENOENT") return fallback;
			throw error;
		}
	};
	const writeJson = (path, value, mode = 0o600) => {
		mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
		const temp = `${path}.tmp`;
		writeFileSync(temp, `${JSON.stringify(value, null, 2)}\n`, { mode });
		renameSync(temp, path);
		chmodSync(path, mode);
	};
	const splitArgs = text => {
		const out = [];
		let current = "";
		let quote = "";
		for (const ch of text) {
			if (quote) {
				if (ch === quote) quote = "";
				else current += ch;
			} else if (ch === '"' || ch === "'") {
				quote = ch;
			} else if (/\s/.test(ch)) {
				if (current) out.push(current);
				current = "";
			} else {
				current += ch;
			}
		}
		if (current) out.push(current);
		return out;
	};
	const marketplaceHelp = [
		"Marketplace commands:",
		"  /marketplace                              List configured sources",
		"  /marketplace add <path|url> [--name n]    Add a catalog source",
		"  /marketplace remove <name>                Remove a source",
		"  /marketplace update [name]                 Fetch catalog(s)",
		"  /marketplace discover [name]               Browse cached plugins",
		"  /marketplace install <name@source>         npm install into the profile",
		"  /marketplace uninstall <name@source>       npm uninstall from the profile",
		"  /marketplace installed                     List profile dependencies",
		"  /marketplace upgrade [name@source]         npm update the profile",
		"  /marketplace help                          Show this guide",
	].join("\n");
	const sourceFilePath = location =>
		location.startsWith("file:") ? fileURLToPath(location) : location;
	const sourceName = location => {
		const raw = basename(sourceFilePath(location)).replace(/\.(json|ya?ml)$/i, "").replace(/\.git$/, "");
		const clean = raw.replace(/[^A-Za-z0-9_.-]/g, "-");
		return clean.length === 0 ? "source" : clean;
	};
	const fetchCatalog = async location => {
		let text;
		if (/^https?:\/\//.test(location)) {
			const response = await fetch(location);
			if (!response.ok) fail("marketplace/fetch", `${response.status} ${response.statusText} for ${location}`);
			text = await response.text();
		} else {
			text = readFileSync(sourceFilePath(location), "utf8");
		}
		let document;
		try {
			document = JSON.parse(text);
		} catch (error) {
			fail("marketplace/catalog", `invalid catalog ${location}: ${messageOf(error)}`);
		}
		const plugins = Array.isArray(document) ? document : document?.plugins;
		if (!Array.isArray(plugins)) fail("marketplace/catalog", `catalog ${location} has no plugins array`);
		return plugins
			.filter(plugin => plugin && typeof plugin === "object" && typeof plugin.name === "string")
			.map(plugin => ({
				name: plugin.name,
				description: typeof plugin.description === "string" ? plugin.description : "",
				package: typeof plugin.package === "string" ? plugin.package : plugin.name,
				version: typeof plugin.version === "string" ? plugin.version : "",
				bundle: typeof plugin.bundle === "string" ? plugin.bundle : "",
			}));
	};
	const readSources = () => {
		const document = readJson(marketplacesPath, { sources: {} });
		return document && typeof document.sources === "object" && document.sources !== null
			? document.sources
			: {};
	};
	const cachedPlugin = async (name, source) => {
		const cache = readJson(join(marketplaceCache, `${name}.json`), null);
		if (!Array.isArray(cache)) {
			fail("marketplace/stale", `source "${name}" is not updated; run /marketplace update ${name}`);
		}
		const plugin = cache.find(entry => entry.name === source);
		if (!plugin) fail("marketplace/not-found", `plugin "${source}" is not in ${name}`);
		return plugin;
	};
	const updateSource = async (name, location) => {
		const plugins = await fetchCatalog(location);
		writeJson(join(marketplaceCache, `${name}.json`), plugins);
		return plugins.length;
	};
	const runNpm = async args => {
		try {
			const result = await promisify(execFile)(process.env.BOOTH_NPM_BIN ?? "npm", args, {
				timeout: 120_000,
				maxBuffer: 16 * 1024 * 1024,
			});
			return (result.stdout || result.stderr || "npm completed").trim();
		} catch (error) {
			const detail = error?.stderr?.toString().trim() || messageOf(error);
			fail("marketplace/npm", detail);
		}
	};
	const readProfilePackage = () => {
		const path = join(profileDir, "package.json");
		const document = readJson(path, null);
		if (!document || typeof document !== "object") fail("marketplace/profile", `no package.json at ${profileDir}`);
		return document;
	};
	const addBundle = bundle => {
		if (!bundle) return;
		const path = join(profileDir, "package.json");
		const document = readProfilePackage();
		document.dsh ??= {};
		document.dsh.profile ??= {};
		document.dsh.profile.bundles ??= [];
		if (!document.dsh.profile.bundles.includes(bundle)) document.dsh.profile.bundles.push(bundle);
		writeJson(path, document, 0o644);
	};
	const removeBundle = bundle => {
		if (!bundle) return;
		const path = join(profileDir, "package.json");
		const document = readProfilePackage();
		const bundles = document.dsh?.profile?.bundles;
		if (!Array.isArray(bundles)) return;
		document.dsh.profile.bundles = bundles.filter(value => value !== bundle);
		writeJson(path, document, 0o644);
	};

	let sideMode;
	const startSideActivation = async (agent, prompt, label, signal) => {
		const common = { provider: "spawn", label, delivery: "caller", signal };
		const nested = { ...common, request: { parent: agent, prompt } };
		const flat = { ...common, parent: agent, prompt };
		if (sideMode === "flat") return ctx.subagents.startActivation(flat);
		if (sideMode === "nested") return ctx.subagents.startActivation(nested);
		try {
			const activation = await ctx.subagents.startActivation(nested);
			sideMode = "nested";
			return activation;
		} catch (error) {
			// dsh 0.2.2+ flattened the request; retry only for shape failures.
			if (!/parent|prompt|maxDepth|reading 'id'|reading 'request'/i.test(messageOf(error))) throw error;
			const activation = await ctx.subagents.startActivation(flat);
			sideMode = "flat";
			return activation;
		}
	};

	const requireService = key => {
		const service = ctx.get(key);
		if (service === undefined) fail("control/unavailable", `this profile mounts no ${key} service`);
		return service;
	};

	const liveAgent = sessionId => {
		if (typeof sessionId !== "string" || sessionId.length === 0) {
			fail("control/bad-request", "sessionId is required");
		}
		const agent = ctx.agents.get(sessionId);
		if (agent === undefined) fail("session/not-live", `session "${sessionId}" is not live`);
		return agent;
	};

	const agentTitle = agent => ctx.get("sessionTitle")?.get(agent.session)?.title ?? "";

	/** One command execution; the handler owns its own cancellation. */
	async function executeCommand(params) {
		const agent = liveAgent(params.sessionId);
		const line = params.line;
		if (typeof line !== "string" || !line.startsWith("/")) {
			fail("control/bad-request", "line must be a slash command");
		}
		const controller = new AbortController();
		const operation = (async () => {
			const commands = requireService("commands");
			const execution = await commands.execute(agent, line, [], controller.signal);
			if (execution === undefined) {
				return { commandId: null, result: { kind: "error", text: `unknown command ${line}` } };
			}
			return { commandId: execution.commandId, result: sanitizeResult(execution.result) };
		})();
		active.add(operation);
		try {
			return await operation;
		} finally {
			active.delete(operation);
		}
	}

	const featuresPath = join(stateDir, "features.json");
	const featureSpecs = {
		browser: [
			"@deepseek-ai/dsh-browser-use@0.2.1-rc.3",
			"@deepseek-ai/dsh-experimental-browser-use-playwright-mcp@0.2.1-rc.3",
		],
		computer: [
			"@deepseek-ai/dsh-computer-use@0.2.1-rc.3",
			"@deepseek-ai/dsh-experimental-computer-use-cua-driver-native@0.2.1-rc.3",
		],
	};
	const readFeatures = () => {
		const document = readJson(featuresPath, {});
		return {
			browser: {
				enabled: document?.browser?.enabled === true,
				headless: document?.browser?.headless !== false,
			},
			computer: { enabled: document?.computer?.enabled === true },
		};
	};
	const writeFeatures = features => writeJson(featuresPath, features);
	const featurePackageName = spec => {
		const at = spec.lastIndexOf("@");
		return at > 0 ? spec.slice(0, at) : spec;
	};
	const featureInstalled = kind => {
		const dependencies = readProfilePackage().dependencies ?? {};
		return featureSpecs[kind].every(spec => dependencies[featurePackageName(spec)] !== undefined);
	};
	const featureStatus = (kind, features) => {
		const state = features[kind];
		const installed = featureInstalled(kind);
		if (kind === "browser") {
			return `browser: ${state.enabled ? "on" : "off"} · ${state.headless ? "headless" : "visible"} · installed: ${installed ? "yes" : "no"}\npackages: ${featureSpecs.browser.join(", ")}\nrestart with /reload-plugins after changes`;
		}
		return `computer: ${state.enabled ? "on" : "off"} · installed: ${installed ? "yes" : "no"}\npackages: ${featureSpecs.computer.join(", ")}\nrestart with /reload-plugins after changes`;
	};
	const featureCommand = async (kind, args) => {
		const tokens = splitArgs(args);
		const verb = tokens[0] ?? "status";
		const features = readFeatures();
		if (verb === "status") return { text: featureStatus(kind, features) };
		if (verb === "help") return { text: `Usage: /${kind} [status|install|on|off${kind === "browser" ? "|headless|visible" : ""}]` };
		if (verb === "install") {
			await runNpm(["install", "--prefix", profileDir, "--save-exact", "--no-audit", "--no-fund", ...featureSpecs[kind]]);
			features[kind].enabled = true;
			writeFeatures(features);
			return { text: `${kind} provider installed and enabled; restart with /reload-plugins.` };
		}
		if (verb === "on" || verb === "off") {
			if (verb === "on" && !featureInstalled(kind)) return { text: `${kind} provider is not installed; run /${kind} install first.` };
			features[kind].enabled = verb === "on";
			writeFeatures(features);
			return { text: `${kind} ${verb === "on" ? "enabled" : "disabled"}; restart with /reload-plugins.` };
		}
		if (kind === "browser" && (verb === "headless" || verb === "visible")) {
			features.browser.headless = verb === "headless";
			writeFeatures(features);
			return { text: `browser mode set to ${verb}; restart with /reload-plugins.` };
		}
		return { text: `Usage: /${kind} [status|install|on|off${kind === "browser" ? "|headless|visible" : ""}]` };
	};

	const securityDir = join(stateDir, "security");
	const securityIndexPath = join(securityDir, "scans.json");
	const securityRuns = new Map();
	const securityHelp = [
		"Security scan commands:",
		"  /security scan [request] [--all]   Start a security audit subagent",
		"  /security status [id]              Show running/completed scans",
		"  /security scans                    List stored scans",
		"  /security show <id>                Print a scan report",
		"  /security cancel [id]              Cancel a running scan",
		"  /security plan [request]           Print the audit plan",
		"  /security help                     Show this guide",
	].join("\n");
	const readSecurityIndex = () => {
		const document = readJson(securityIndexPath, { scans: {} });
		return document && typeof document.scans === "object" && document.scans !== null
			? document
			: { scans: {} };
	};
	const writeSecurityIndex = document => writeJson(securityIndexPath, document);
	const securityText = result =>
		(result.output ?? [])
			.filter(block => block?.type === "text" && typeof block.text === "string")
			.map(block => block.text)
			.join("") || "Security scan finished without a report.";

	const prewalkArmed = new Map();
	const prewalkSwitches = new Map();
	const resolvePrewalkTarget = async text => {
		const selector = text.trim();
		if (selector.length === 0) fail("prewalk/bad-request", "a model selector is required");
		const providers = ctx.llm.listProviders();
		const slash = selector.indexOf("/");
		if (slash > 0) {
			const provider = selector.slice(0, slash);
			const model = selector.slice(slash + 1);
			if (providers.some(entry => entry.id === provider)) {
				const models = await ctx.llm.listModels(provider);
				const found = models.find(entry => entry.id === model);
				if (found) return { provider, model: found.id, label: `${provider}/${found.id}` };
			}
		}
		for (const provider of providers) {
			let models = [];
			try {
				models = await ctx.llm.listModels(provider.id);
			} catch {
				continue;
			}
			const found = models.find(entry => entry.id === selector || entry.name.toLowerCase() === selector.toLowerCase());
			if (found) return { provider: provider.id, model: found.id, label: `${provider.id}/${found.id}` };
		}
		fail("prewalk/not-found", `model "${selector}" not found`);
	};
	const prewalkStatus = agent => {
		const switched = prewalkSwitches.get(agent.id);
		if (switched) return `Prewalk active: ${switched.target.label} for this turn.`;
		const armed = prewalkArmed.get(agent.id);
		return armed === undefined ? "Prewalk off." : `Prewalk armed: ${armed.label} at the next user turn.`;
	};
	ctx.on?.("agent/inbox/inserted", ({ agent, message }) => {
		if (message?.source?.kind !== "user") return;
		const armed = prewalkArmed.get(agent.id);
		if (armed === undefined) return;
		prewalkArmed.delete(agent.id);
		const previous = agent.model.current?.();
		agent.model.select({ provider: armed.provider, model: armed.model });
		prewalkSwitches.set(agent.id, { target: armed, previous });
	});
	ctx.on?.("agent/turn-stopping", ({ agent }) => {
		const switched = prewalkSwitches.get(agent.id);
		if (switched === undefined) return;
		prewalkSwitches.delete(agent.id);
		if (switched.previous !== undefined) agent.model.select(switched.previous);
	});

	const advisorEnabled = new Map();
	const advisorNotes = new Map();
	const advisorRuns = new Map();
	const advisorText = context => {
		const id = context?.session?.id ?? context?.session?.header?.id;
		const notes = advisorNotes.get(id) ?? [];
		return notes.length === 0
			? ""
			: `<booth-advisor>\nAdvisor notes from earlier turns:\n${notes.map(note => `- ${note}`).join("\n")}\n</booth-advisor>`;
	};
	ctx.systemPrompt.context({
		name: "booth-advisor",
		order: 242,
		interpolate: false,
		text: advisorText,
	});
	const textBlocks = data => {
		const content = data?.content ?? data?.message?.content ?? [];
		return content
			.filter(block => block?.type === "text" && typeof block.text === "string")
			.map(block => block.text)
			.join("")
			.trim();
	};
	const loadTurnText = async agent => {
		const observation = await ctx.sessionQuery.observeSession(agent.id);
		try {
			const events = observation.events ?? [];
			const messages = events
				.filter(event => event.type === "user/message" || event.type === "assistant/message")
				.slice(-6)
				.map(event => `${event.type === "user/message" ? "User" : "Assistant"}: ${textBlocks(event.data)}`)
				.filter(line => line.length > 9);
			return messages.join("\n\n").slice(-6000) || "(no recent messages)";
		} finally {
			observation[Symbol.dispose]?.();
		}
	};
	const reviewAdvisor = async agent => {
		if (advisorRuns.has(agent.id)) return;
		const run = (async () => {
			try {
				const transcript = await loadTurnText(agent);
				const controller = new AbortController();
				const activation = await startSideActivation(agent, [{
					type: "text",
					text: [
						"You are the advisor. Review the last turn for correctness, missing tests, risky assumptions, and the single highest-value next action.",
						"Do not modify files. Return 3 to 6 terse bullets, each actionable.",
						"",
						transcript,
					].join("\n"),
				}], "advisor", controller.signal);
				try {
					const result = await activation.result;
					const advice = (result.output ?? [])
						.filter(block => block?.type === "text" && typeof block.text === "string")
						.map(block => block.text)
						.join("")
						.trim();
					if (advice.length > 0) {
						const notes = advisorNotes.get(agent.id) ?? [];
						notes.push(advice);
						advisorNotes.set(agent.id, notes.slice(-8));
						ctx.emit?.("system-prompt/change");
					}
				} finally {
					await activation.dispose();
				}
			} catch (error) {
				ctx.logger?.warn?.(`booth advisor: ${messageOf(error)}`);
			} finally {
				advisorRuns.delete(agent.id);
			}
		})();
		advisorRuns.set(agent.id, run);
	};
	ctx.on?.("agent/turn-stopping", ({ agent }) => {
		if (advisorEnabled.get(agent.id) === true) void reviewAdvisor(agent);
	});

	const forceArmed = new Map();
	const forceRuns = new Map();
	const forceStatus = agent => {
		if (forceRuns.has(agent.id)) return `Force active: only "${forceRuns.get(agent.id).tool}" is visible this turn.`;
		const armed = forceArmed.get(agent.id);
		return armed === undefined ? "Force: off." : `Force armed: "${armed.tool}" at the next user turn.`;
	};
	ctx.on?.("agent/inbox/inserted", ({ agent, message }) => {
		if (message?.source?.kind !== "user") return;
		const armed = forceArmed.get(agent.id);
		if (armed === undefined) return;
		forceArmed.delete(agent.id);
		try {
			const names = ctx.tools.definitions(agent).map(definition => definition.name);
			if (!names.includes(armed.tool)) {
				ctx.logger?.warn?.(`booth force: tool "${armed.tool}" is no longer available`);
				return;
			}
			const dispose = agent.ctx.tools.restrict({ allow: [armed.tool] });
			forceRuns.set(agent.id, { tool: armed.tool, dispose });
		} catch (error) {
			ctx.logger?.warn?.(`booth force: ${messageOf(error)}`);
		}
	});
	ctx.on?.("agent/turn-stopping", ({ agent }) => {
		const run = forceRuns.get(agent.id);
		if (run === undefined) return;
		forceRuns.delete(agent.id);
		try {
			run.dispose();
		} catch (error) {
			ctx.logger?.warn?.(`booth force restore: ${messageOf(error)}`);
		}
	});

	const extendedPrevious = new Map();
	const skillfulOff = new Map();
	const skillfulRestrictions = new Map();
	const skillfulText = context => {
		const id = context?.session?.id ?? context?.session?.header?.id;
		return skillfulOff.get(id) === true
			? "<booth-skills-off>\nThe session skill catalog is disabled. Do not invoke skills or treat the catalog as available.\n</booth-skills-off>"
			: "";
	};
	ctx.systemPrompt.context({
		name: "booth-skills-off",
		order: 243,
		interpolate: false,
		text: skillfulText,
	});
	const setSkillful = (agent, enabled) => {
		if (!enabled) {
			if (!skillfulRestrictions.has(agent.id)) {
				const names = ctx.tools.definitions(agent).map(definition => definition.name);
				if (names.includes("skill")) {
					const dispose = agent.ctx.tools.restrict({ deny: ["skill"] });
					skillfulRestrictions.set(agent.id, dispose);
				}
			}
			skillfulOff.set(agent.id, true);
		} else {
			const dispose = skillfulRestrictions.get(agent.id);
			skillfulRestrictions.delete(agent.id);
			skillfulOff.delete(agent.id);
			if (dispose !== undefined) dispose();
		}
		ctx.emit?.("system-prompt/change");
	};

	const shares = new Map();
	const shareSnapshot = async agent => {
		const observation = await ctx.sessionQuery.observeSession(agent.id);
		try {
			const entries = [];
			for (const event of observation.events ?? []) {
				if (event.type !== "user/message" && event.type !== "assistant/message") continue;
				const text = textBlocks(event.data);
				if (text.length > 0) {
					entries.push({
						role: event.type === "user/message" ? "user" : "assistant",
						time: event.time,
						text: text.slice(0, 12000),
					});
				}
			}
			const title = ctx.get("sessionTitle")?.get(agent.session)?.title ?? "";
			return { sessionId: agent.id, title, status: agent.status, entries: entries.slice(-200) };
		} finally {
			observation[Symbol.dispose]?.();
		}
	};
	const viewerHtml = token => `<!doctype html><html lang="en"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>booth share</title>
<style>body{font:14px/1.5 system-ui,sans-serif;margin:2rem;max-width:60rem;background:#161616;color:#ddd}
h1{font-size:1.2rem}main{white-space:pre-wrap}article{border-bottom:1px solid #333;padding:.7rem 0}
.role{color:#8bd5ff;font-size:12px;text-transform:uppercase}form{display:flex;gap:.5rem;margin-top:1.5rem}
textarea{flex:1;min-height:3rem;background:#222;color:#eee;border:1px solid #444;padding:.5rem}
button{background:#2d6cdf;color:white;border:0;padding:.5rem 1rem}button:disabled{opacity:.4}
#status{color:#888}</style><h1 id="title">booth share</h1><div id="meta"></div><main id="log"></main>
<form id="prompt"><textarea id="text" placeholder="Message the shared session"></textarea><button>Send</button></form>
<p id="status"></p><script>
const token=${JSON.stringify(token)};
const log=document.querySelector('#log');const title=document.querySelector('#title');const meta=document.querySelector('#meta');
const form=document.querySelector('#prompt');const text=document.querySelector('#text');const status=document.querySelector('#status');const button=document.querySelector('button');
async function refresh(){try{const r=await fetch('/api/snapshot?token='+encodeURIComponent(token));if(!r.ok)throw new Error('HTTP '+r.status);const d=await r.json();
title.textContent=d.title||'booth share';meta.textContent=d.status+' · '+d.entries.length+' messages';log.replaceChildren(...d.entries.map(e=>{const a=document.createElement('article');const role=document.createElement('div');role.className='role';role.textContent=e.role;a.append(role,document.createTextNode(e.text));return a;}));
button.disabled=!d.write;status.textContent=d.write?'shared writing is on':'read-only';}catch(e){status.textContent='disconnected';}}
form.addEventListener('submit',async e=>{e.preventDefault();if(!text.value.trim())return;const r=await fetch('/api/prompt',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({token,text:text.value})});status.textContent=r.ok?'sent':('failed: '+r.status);if(r.ok)text.value='';refresh();});
refresh();setInterval(refresh,1500);
</script></html>`;
	const startShare = async (agent, write) => {
		const token = randomUUID().replace(/-/g, "");
		const share = { token, agent, write, server: undefined, url: "" };
		const server = createHttpServer((request, response) => {
			void (async () => {
				const url = new URL(request.url ?? "/", "http://127.0.0.1");
				const supplied = url.searchParams.get("token") ?? request.headers["x-booth-token"];
				response.setHeader("cache-control", "no-store");
				if (request.method === "GET" && url.pathname === `/s/${token}`) {
					response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
					response.end(viewerHtml(token));
					return;
				}
				if (supplied !== token) {
					response.writeHead(403, { "content-type": "text/plain" });
					response.end("forbidden");
					return;
				}
				if (request.method === "GET" && url.pathname === "/api/snapshot") {
					response.writeHead(200, { "content-type": "application/json" });
					response.end(JSON.stringify({ ...(await shareSnapshot(agent)), write: share.write }));
					return;
				}
				if (request.method === "POST" && url.pathname === "/api/prompt") {
					if (!share.write) {
						response.writeHead(403, { "content-type": "text/plain" });
						response.end("read-only share");
						return;
					}
					let body = "";
					for await (const chunk of request) {
						body += chunk;
						if (body.length > 65536) {
							response.writeHead(413, { "content-type": "text/plain" });
							response.end("prompt too large");
							return;
						}
					}
					let document;
					try {
						document = JSON.parse(body);
					} catch {
						response.writeHead(400, { "content-type": "text/plain" });
						response.end("invalid JSON");
						return;
					}
					const text = typeof document?.text === "string" ? document.text.trim() : "";
					if (text.length === 0) {
						response.writeHead(400, { "content-type": "text/plain" });
						response.end("empty prompt");
						return;
					}
					const { createUserMessage } = await harnessModule("@deepseek-ai/dsh-llm");
					agent.followup(createUserMessage({
						content: [{ type: "text", text: `[shared viewer] ${text}` }],
						source: { kind: "user" },
					}));
					response.writeHead(202, { "content-type": "application/json" });
					response.end('{"queued":true}');
					return;
				}
				response.writeHead(404, { "content-type": "text/plain" });
				response.end("not found");
			})().catch(error => {
				if (!response.headersSent) response.writeHead(500, { "content-type": "text/plain" });
				response.end("server error");
				ctx.logger?.warn?.(`booth share: ${messageOf(error)}`);
			});
		});
		await new Promise((resolve, reject) => {
			server.once("error", reject);
			server.listen(0, "127.0.0.1", () => {
				server.off("error", reject);
				resolve();
			});
		});
		const address = server.address();
		share.server = server;
		share.url = `http://127.0.0.1:${address.port}/s/${token}`;
		shares.set(agent.id, share);
		return share;
	};
	const stopShare = async agent => {
		const share = shares.get(agent.id);
		if (share === undefined) return false;
		shares.delete(agent.id);
		await new Promise(resolve => share.server.close(() => resolve()));
		return true;
	};

	const vibeEnabled = new Map();
	const serviceTiers = new Map();
	ctx.on?.("agent/request", async ({ agent }, next) => {
		const config = await next();
		const tier = serviceTiers.get(agent.id);
		if (tier === undefined) return config;
		return {
			...config,
			samplingParams: { ...(config.samplingParams ?? {}), service_tier: tier },
		};
	});
	const setServiceTier = (agent, tier) => {
		if (tier === undefined) serviceTiers.delete(agent.id);
		else serviceTiers.set(agent.id, tier);
	};

		const providerDescriptor = () => {
			const descriptor = ctx.settings.describe({ redactSecrets: true }).find(item => item.ns === "llm-pi-ai");
			if (descriptor === undefined) fail("providers/unavailable", "this profile exposes no llm-pi-ai settings");
			return descriptor;
		};
		const providerDirectory = () => {
			try {
				const rows = ctx.llm?.listConfigurableProviders?.() ?? [];
				return rows.filter(row => row.settingsNs === "llm-pi-ai");
			} catch {
				return [];
			}
		};
		const providerProfile = (descriptor, provider) => {
			const providers = descriptor.value && typeof descriptor.value === "object" ? descriptor.value.providers : undefined;
			const profile = providers && typeof providers === "object" ? providers[provider] : undefined;
			return profile && typeof profile === "object" ? profile : undefined;
		};
		const providerUserProfile = (descriptor, provider) => {
			const providers = descriptor.user && typeof descriptor.user === "object" ? descriptor.user.providers : undefined;
			const profile = providers && typeof providers === "object" ? providers[provider] : undefined;
			return profile && typeof profile === "object" ? profile : undefined;
		};
		const apiKeyRef = provider => `${String(provider).toUpperCase().replace(/[^A-Z0-9]+/g, "_")}_API_KEY`;
		const providerView = async provider => {
			const descriptor = providerDescriptor();
			const profile = providerProfile(descriptor, provider);
			const directory = providerDirectory().find(row => row.provider === provider);
			const ref = typeof profile?.apiKeyEnv === "string" && profile.apiKeyEnv.length > 0 ? profile.apiKeyEnv : apiKeyRef(provider);
			let key = { configured: false, writable: true };
			try {
				key = await ctx.credentials.describe(ref);
			} catch {
				// A provider may be configuration-only until its credential service is mounted.
			}
			return {
				provider,
				displayName: typeof profile?.displayName === "string" && profile.displayName.length > 0
					? profile.displayName
					: directory?.displayName ?? provider,
				baseURL: typeof profile?.baseURL === "string" ? profile.baseURL : "",
				defaultBaseURL: typeof directory?.defaultBaseURL === "string" ? directory.defaultBaseURL : "",
				api: typeof profile?.api === "string" ? profile.api : "",
				models: Array.isArray(profile?.models)
					? profile.models.map(model => typeof model === "string" ? model : model?.id).filter(id => typeof id === "string")
					: [],
				configured: profile !== undefined,
				declared: directory?.declared === true,
				removable: providerUserProfile(descriptor, provider) !== undefined,
				error: typeof directory?.error === "string" ? directory.error : "",
				apiKeyEnv: ref,
				apiKeyConfigured: key.configured === true,
				apiKeyWritable: key.writable !== false,
				apiKeyMasked: typeof key.masked === "string" ? key.masked : "",
				revision: descriptor.revision,
			};
		};

	const handlers = {
		async hello() {
			return {
				protocol: PROTOCOL_VERSION,
				socket: socketPath,
				sessions: ctx.agents.list().map(agent => agent.id),
			};
		},

		async "command.list"(params) {
			const agent = liveAgent(params.sessionId);
			const commands = requireService("commands");
			return commands.list(agent).map(command => ({
				name: command.name,
				description: command.description ?? "",
				hint: command.input?.hint ?? "",
				aliases: command.aliases ?? [],
			}));
		},

		"command.execute": executeCommand,

		async "session.rename"(params) {
			const agent = liveAgent(params.sessionId);
			const titles = requireService("sessionTitle");
			const requested = typeof params.title === "string" ? params.title.trim() : "";
			const snapshot =
				requested.length === 0 ? await titles.refresh(agent.session) : titles.rename(agent.session, requested);
			if (snapshot === undefined) fail("session/title-unavailable", "this session has no title yet");
			return { title: snapshot.title, seq: Number(snapshot.eventSeq) };
		},

		async "session.delete"(params) {
			const sessionId = params.sessionId;
			if (typeof sessionId !== "string" || sessionId.length === 0) {
				fail("control/bad-request", "sessionId is required");
			}
			if (ctx.agents.get(sessionId) !== undefined) {
				fail("session/not-idle", `session "${sessionId}" is live; close it before deleting`);
			}
			const persistence = requireService("sessionPersistence");
			const observation = await ctx.sessionQuery.observeSession(sessionId);
			let header;
			try {
				header = observation.header;
			} finally {
				observation[Symbol.dispose]?.();
			}
			const located = typeof persistence.locate === "function" ? persistence.locate(header) : undefined;
			if (located?.kind !== "jsonl" || typeof located.path !== "string") {
				fail("session/delete-unsupported", "this persistence backend does not expose session deletion");
			}
			rmSync(dirname(located.path), { recursive: true, force: true });
			return { deleted: true, sessionId };
		},

		async "session.fork"(params) {
			const sessionId = params.sessionId;
			if (typeof sessionId !== "string" || sessionId.length === 0) {
				fail("control/bad-request", "sessionId is required");
			}
			const requested = params.atSeq;
			if (requested !== undefined && (!Number.isSafeInteger(requested) || requested < 0)) {
				fail("control/bad-request", "atSeq must be a non-negative safe integer");
			}
			const observation = await ctx.sessionQuery.observeSession(sessionId);
			try {
				const boundary = requested ?? latestBoundary(observation.events);
				if (boundary === undefined || observation.events[boundary]?.seq !== boundary) {
					fail(
						"session/fork-unavailable",
						requested === undefined
							? `session "${sessionId}" has no completed turn to fork from`
							: `event ${String(requested)} does not exist in session "${sessionId}"`,
					);
				}
				const { buildForkSeed } = await harnessModule("@deepseek-ai/dsh-session/fork");
				const sessions = await harnessModule("@deepseek-ai/dsh-session");
				const seed = buildForkSeed(observation.events, boundary);
				const childId = `session-${randomUUID()}`;
				const meta = {
					...(observation.header.cwd === undefined ? {} : { cwd: observation.header.cwd }),
					isSeeded: true,
					...(observation.header.agentPreset === undefined
						? {}
						: { agentPreset: observation.header.agentPreset }),
				};
				const header =
					typeof sessions.buildSessionHeader === "function"
						? sessions.buildSessionHeader(childId, meta)
						: {
								version: sessions.SESSION_FORMAT_VERSION ?? 4,
								id: childId,
								createdAt: Date.now(),
								...meta,
							};
				const persistence = requireService("sessionPersistence");
				// Older backends ignore `parentSource`; create the seeded header and
				// append the complete fork seed so either implementation ends with a
				// self-contained child log.
				const handle = await persistence.create(header, { inheritedEventCount: boundary + 1 });
				try {
					await handle.append(seed);
					await handle.flush();
				} finally {
					await handle.close?.();
				}
				const inheritedTitle = observation.events.findLast(event => event.type === "session/title")?.data?.title ?? "";
				return { sessionId: childId, atSeq: boundary, title: inheritedTitle, cwd: observation.header.cwd ?? "" };
			} finally {
				observation[Symbol.dispose]?.();
			}
		},

		async "session.events"(params) {
			const sessionId = params.sessionId;
			if (typeof sessionId !== "string" || sessionId.length === 0) {
				fail("control/bad-request", "sessionId is required");
			}
			const observation = await ctx.sessionQuery.observeSession(sessionId);
			try {
				const limit = Number.isSafeInteger(params.limit) && params.limit > 0 ? params.limit : 500;
				const events = observation.events;
				const start = Math.max(0, events.length - limit);
				return events.slice(start).map(event => {
					const row = { seq: Number(event.seq), type: event.type };
					if (event.type === "user/message") {
						const content = Array.isArray(event.data?.content) ? event.data.content : [];
						const text = content
							.filter(block => block?.type === "text" && typeof block.text === "string")
							.map(block => block.text)
							.join(" ");
						if (text.length > 0) row.text = text.slice(0, 400);
					}
					if (event.type === "assistant/message") {
						const content = Array.isArray(event.data?.message?.content) ? event.data.message.content : [];
						const text = content
							.filter(block => block?.type === "text" && typeof block.text === "string")
							.map(block => block.text)
							.join(" ");
						if (text.length > 0) row.text = text.slice(0, 400);
					}
					return row;
				});
			} finally {
				observation[Symbol.dispose]?.();
			}
		},

		async "dirs.list"(params) {
			const agent = liveAgent(params.sessionId);
			const policy = ctx.sandboxPolicy.resolve({ session: agent.session });
			return {
				cwd: agent.session.header.cwd ?? "",
				roots: [...(policy.additionalWritableRoots ?? [])],
			};
		},

		async "dirs.add"(params) {
			const agent = liveAgent(params.sessionId);
			if (typeof ctx.sandboxPolicy.registerWritableRoot !== "function") {
				fail("dirs/unsupported", "this dsh build has no session writable roots; upgrade dsh to 0.2.3 or newer");
			}
			const requested = params.path;
			if (typeof requested !== "string" || !requested.startsWith("/")) {
				fail("dirs/invalid", "an absolute directory path is required");
			}
			let canonical;
			try {
				canonical = realpathSync(requested);
				if (!statSync(canonical).isDirectory()) fail("dirs/invalid", `${requested} is not a directory`);
			} catch (error) {
				if (error instanceof ControlError) throw error;
				fail("dirs/missing", `cannot resolve ${requested}: ${messageOf(error)}`);
			}
			let grants = sessionDirs.get(agent.session.id);
			if (grants === undefined) {
				grants = new Map();
				sessionDirs.set(agent.session.id, grants);
			}
			if (!grants.has(canonical)) {
				grants.set(canonical, ctx.sandboxPolicy.registerWritableRoot(agent.session, canonical));
			}
			return { path: canonical, roots: [...grants.keys()] };
		},

		async "dirs.remove"(params) {
			const agent = liveAgent(params.sessionId);
			const requested = params.path;
			if (typeof requested !== "string" || requested.length === 0) {
				fail("control/bad-request", "path is required");
			}
			const grants = sessionDirs.get(agent.session.id);
			let key = requested;
			try {
				key = realpathSync(requested);
			} catch {
				// A removed directory still needs its runtime grant released.
			}
			const dispose = grants?.get(key);
			if (dispose === undefined) fail("dirs/not-found", `${requested} is not an added directory`);
			grants.delete(key);
			dispose();
			return { path: key, roots: [...(grants?.keys() ?? [])] };
		},

		async "session.move"(params) {
			const sessionId = params.sessionId;
			const requested = params.path;
			if (typeof sessionId !== "string" || sessionId.length === 0) {
				fail("control/bad-request", "sessionId is required");
			}
			if (typeof requested !== "string" || !requested.startsWith("/")) {
				fail("move/invalid", "an absolute target directory is required");
			}
			if (ctx.agents.get(sessionId) !== undefined) {
				fail("session/not-idle", "close the live session before moving it");
			}
			let target;
			try {
				target = realpathSync(requested);
				if (!statSync(target).isDirectory()) fail("move/invalid", `${requested} is not a directory`);
			} catch (error) {
				if (error instanceof ControlError) throw error;
				fail("move/missing", `cannot resolve ${requested}: ${messageOf(error)}`);
			}
			const observation = await ctx.sessionQuery.observeSession(sessionId);
			try {
				if (observation.header.cwd === target) fail("move/noop", `session is already in ${target}`);
				const childId = `session-${randomUUID()}`;
				const meta = {
					cwd: target,
					...(observation.header.agentPreset === undefined
						? {}
						: { agentPreset: observation.header.agentPreset }),
				};
				const persistence = requireService("sessionPersistence");
				if (observation.events.length === 0) {
					const header = {
						version: 4,
						id: childId,
						createdAt: Date.now(),
						isSeeded: false,
						...meta,
					};
					const empty = await persistence.create(header, {});
					try {
						await empty.flush();
					} finally {
						await empty.close?.();
					}
				} else {
					const { buildForkSeed } = await harnessModule("@deepseek-ai/dsh-session/fork");
					const sessions = await harnessModule("@deepseek-ai/dsh-session");
					const boundary = observation.events.length - 1;
					const seed = buildForkSeed(observation.events, boundary);
					const header =
						typeof sessions.buildSessionHeader === "function"
							? sessions.buildSessionHeader(childId, { ...meta, isSeeded: true })
							: {
									version: sessions.SESSION_FORMAT_VERSION ?? 4,
									id: childId,
									createdAt: Date.now(),
									isSeeded: true,
									...meta,
								};
					const handle = await persistence.create(header, { inheritedEventCount: boundary + 1 });
					try {
						await handle.append(seed);
						await handle.flush();
					} finally {
						await handle.close?.();
					}
				}
				const located = typeof persistence.locate === "function"
					? persistence.locate(observation.header)
					: undefined;
				if (located?.kind === "jsonl" && typeof located.path === "string") {
					rmSync(dirname(located.path), { recursive: true, force: true });
				}
				const title = observation.events.findLast(event => event.type === "session/title")?.data?.title ?? "";
				return { sessionId: childId, cwd: target, title };
			} finally {
				observation[Symbol.dispose]?.();
			}
		},

		async "session.worktree"(params) {
			const sessionId = params.sessionId;
			if (typeof sessionId !== "string" || sessionId.length === 0) {
				fail("control/bad-request", "sessionId is required");
			}
			const agent = ctx.agents.get(sessionId);
			if (agent !== undefined) fail("session/not-idle", "close the live session before moving it");
			const requested = typeof params.branch === "string" ? params.branch.trim() : "";
			const stamp = new Date().toISOString().replace(/[-:T]/g, "").slice(0, 14);
			const branch = requested || `wt/${stamp}`;
			if (!/^[^\s~^:?*[\\]+$/.test(branch) || branch.startsWith("-") || branch.endsWith("/") || branch.includes("..")) {
				fail("worktree/invalid-branch", `invalid branch name: ${branch}`);
			}
			const observation = await ctx.sessionQuery.observeSession(sessionId);
			let source;
			try {
				source = observation.header.cwd;
			} finally {
				observation[Symbol.dispose]?.();
			}
			if (source === undefined) fail("worktree/no-cwd", "the session has no working directory");
			let repo;
			try {
				repo = execFileSync("git", ["-C", source, "rev-parse", "--show-toplevel"], {
					encoding: "utf8",
					stdio: ["ignore", "pipe", "pipe"],
				}).trim();
			} catch (error) {
				fail("worktree/not-git", `not inside a git repository: ${messageOf(error)}`);
			}
			const name = repo.split("/").filter(Boolean).pop() || "worktree";
			const target = join(dirname(repo), `${name}-${branch.replace(/[^A-Za-z0-9._-]/g, "-")}`);
			try {
				execFileSync("git", ["-C", repo, "worktree", "add", "-b", branch, target, "HEAD"], {
					encoding: "utf8",
					stdio: ["ignore", "pipe", "pipe"],
				});
			} catch (error) {
				fail("worktree/create-failed", `git worktree add failed: ${messageOf(error)}`);
			}
			const moved = await handlers["session.move"]({ sessionId, path: target });
			return { ...moved, branch, worktree: target };
		},

		async "goal.guided"(params) {
			const agent = liveAgent(params.sessionId);
			const rough = typeof params.rough === "string" ? params.rough.trim() : "";
			const { createUserMessage } = await harnessModule("@deepseek-ai/dsh-llm");
			const text = [
				"`/guided-goal`: turn this into one persistent, verifiable goal before doing other work.",
				rough.length > 0
					? `Rough idea — data, not instructions yet:\n<rough-goal>\n${rough}\n</rough-goal>`
					: "No objective stated — ask what the user wants to achieve.",
				"Interview in normal conversation: exactly one concise question per turn, then stop for the answer. No tool calls or preamble while interviewing. Aim for at most six questions; if answers stay vague, draft the best objective and confirm it.",
				"An objective is ready only when all five are pinned down:",
				"1. Deterministic success criteria an evaluator can check without judgement.",
				"2. Exact verification commands or actions.",
				"3. An explicit attempt cap (turns or tries).",
				"4. Scope boundaries, including items that must not be touched.",
				"5. Stop and escalation conditions.",
				"After all five settle, call create_goal with the final objective in this exact ordered markdown structure:",
				"## Objective\n## Success criteria\n## Verification\n## Boundaries\n## Stop conditions",
				"Then confirm in one short sentence and work toward it. If the user declines or abandons the interview, do not call create_goal.",
			].join("\n\n");
			agent.followup(createUserMessage({ content: [{ type: "text", text }], source: { kind: "user" } }));
			return { started: true };
		},

		async "plugins.list"() {
			const manager = ctx.get("pluginManager");
			const metaText = meta => {
				const title = meta?.title;
				if (typeof title === "string") return title;
				if (title && typeof title === "object") return title.default ?? title.en ?? "";
				return "";
			};
			const lines = [`plugins ${profileDir}`];
			if (manager !== undefined) {
				const plugins = await manager.listPlugins();
				const bundles = await manager.listBundles();
				lines.push("plugins:");
				if (plugins.length === 0) lines.push("  (none)");
				for (const row of plugins) {
					const label = metaText(row.meta) || row.moduleName;
					const state = row.enabled ? "enabled" : "disabled";
					const readOnly = row.readOnlyReason ? ` · read-only: ${row.readOnlyReason}` : "";
					lines.push(`  ${state} ${label} (${row.moduleName})${readOnly}`);
				}
				lines.push("bundles:");
				if (bundles.length === 0) lines.push("  (none)");
				for (const row of bundles) {
					const state = row.enabled ? "enabled" : "disabled";
					const installed = row.installed === true ? "installed" : "not installed";
					lines.push(`  ${state} ${row.name} · ${installed}`);
				}
			} else {
				const document = readProfilePackage();
				const dependencies = document.dependencies ?? {};
				const bundles = document?.dsh?.profile?.bundles ?? [];
				lines.push("dependencies:");
				const names = Object.keys(dependencies);
				if (names.length === 0) lines.push("  (none)");
				for (const name of names) lines.push(`  ${name} ${dependencies[name]}`);
				lines.push(`bundles: ${bundles.length === 0 ? "(none)" : bundles.join(" ")}`);
			}
			return { text: lines.join("\n") };
		},

		async "plugins.set"(params) {
			const manager = ctx.get("pluginManager");
			if (manager === undefined) fail("plugins/unavailable", "this profile mounts no plugin manager");
			const name = typeof params.name === "string" ? params.name.trim() : "";
			if (name.length === 0) fail("plugins/bad-request", "plugin name is required");
			const enabled = params.enabled === true;
			const metaText = meta => {
				const title = meta?.title;
				if (typeof title === "string") return title;
				if (title && typeof title === "object") return title.default ?? title.en ?? "";
				return "";
			};
			const plugins = await manager.listPlugins();
			const plugin = plugins.find(row =>
				row.entryId === name || row.moduleName === name || metaText(row.meta) === name ||
				String(row.entryId).endsWith(name) || row.moduleName.split("/").at(-1) === name);
			if (plugin !== undefined) {
				const result = await manager.setPluginEnabled(plugin.entryId, enabled);
				const restart = result?.restartReason ? ` · restart required (${result.restartReason})` : "";
				return { text: `plugin ${plugin.moduleName} ${enabled ? "enabled" : "disabled"}${restart}`, result };
			}
			const bundles = await manager.listBundles();
			const bundle = bundles.find(row => row.name === name || metaText(row.meta) === name);
			if (bundle !== undefined) {
				if (bundle.readOnlyReason !== undefined) fail("plugins/read-only", `bundle ${name} is read-only: ${bundle.readOnlyReason}`);
				const result = await manager.setBundleEnabled(bundle.name, enabled);
				const restart = result?.restartReason ? ` · restart required (${result.restartReason})` : "";
				return { text: `bundle ${bundle.name} ${enabled ? "enabled" : "disabled"}${restart}`, result };
			}
			fail("plugins/not-found", `no plugin or bundle named "${name}"`);
		},

		async "memory"(params) {
			const args = typeof params.args === "string" ? params.args.trim() : "";
			const tokens = args.length === 0 ? [] : args.split(/\s+/);
			const verb = tokens[0] ?? "view";
			const rest = args.slice(verb.length).trim();
			const usage =
				"Usage: /memory [view|add <note>|stats|diagnose|queue|sync|clear|reset|enqueue|rebuild]";
			try {
				if (verb === "view") {
					const text = readMemory().trim();
					return { text: text.length === 0 ? "Memory payload is empty." : text };
				}
				if (verb === "add" || verb === "remember") {
					if (rest.length === 0) return { text: "Usage: /memory add <note>" };
					const current = readMemory().trimEnd();
					writeMemory(`${current.length === 0 ? "" : `${current}\n`}${rest}\n`);
					return { text: "Memory note added." };
				}
				if (verb === "stats") {
					let bytes = 0;
					let updated = "never";
					try {
						const info = statSync(memoryPath);
						bytes = info.size;
						updated = info.mtime.toISOString();
					} catch (error) {
						if (error?.code !== "ENOENT") throw error;
					}
					const lines = readMemory().split("\n").filter(line => line.trim().length > 0).length;
					return {
						text: `memory local\npath: ${memoryPath}\nbytes: ${bytes}\nnotes: ${lines}\nupdated: ${updated}`,
					};
				}
				if (verb === "diagnose") {
					const exists = existsSync(memoryPath);
					let bytes = 0;
					if (exists) bytes = statSync(memoryPath).size;
					return {
						text: `memory diagnose: ok\npath: ${memoryPath}\nexists: ${exists}\nbytes: ${bytes}\nbackend: local Markdown`,
					};
				}
				if (verb === "queue") {
					return { text: "No pending memory deltas for the local backend." };
				}
				if (verb === "sync" || verb === "enqueue" || verb === "rebuild") {
					const before = readMemory();
					const lines = [];
					const seen = new Set();
					for (const line of before.split("\n")) {
						const note = line.trim();
						if (note.length === 0 || seen.has(note)) continue;
						seen.add(note);
						lines.push(note);
					}
					const after = lines.length === 0 ? "" : `${lines.join("\n")}\n`;
					writeMemory(after);
					return {
						text: `Memory consolidation complete. Notes: ${lines.length}; bytes: ${before.length} -> ${after.length}.`,
					};
				}
				if (verb === "clear" || verb === "reset") {
					rmSync(memoryPath, { force: true });
					ctx.emit?.("system-prompt/change");
					return { text: "Memory cleared." };
				}
				if (verb === "mm") {
					return { text: "Mental-model maintenance is unavailable for the local memory backend." };
				}
				return { text: usage };
			} catch (error) {
				fail("memory/error", messageOf(error));
			}
		},

		async "marketplace"(params) {
			const args = typeof params.args === "string" ? params.args.trim() : "";
			const tokens = splitArgs(args);
			const verb = tokens[0] ?? "list";
			try {
				if (verb === "help") return { text: marketplaceHelp };
				if (verb === "add") {
					let name;
					let location;
					for (let index = 1; index < tokens.length; index += 1) {
						if (tokens[index] === "--name") {
							name = tokens[index + 1];
							index += 1;
							continue;
						}
						if (location === undefined) location = tokens[index];
						else fail("marketplace/bad-request", `unexpected argument "${tokens[index]}"`);
					}
					if (!location) return { text: "Usage: /marketplace add <path|url> [--name name]" };
					name = name || sourceName(location);
					const sources = readSources();
					sources[name] = { location };
					writeJson(marketplacesPath, { sources });
					const count = await updateSource(name, location);
					return { text: `Added marketplace "${name}" (${count} plugins).` };
				}
				if (verb === "remove") {
					const name = tokens[1];
					if (!name) return { text: "Usage: /marketplace remove <name>" };
					const sources = readSources();
					if (sources[name] === undefined) return { text: `Marketplace "${name}" is not configured.` };
					delete sources[name];
					writeJson(marketplacesPath, { sources });
					rmSync(join(marketplaceCache, `${name}.json`), { force: true });
					return { text: `Removed marketplace "${name}".` };
				}
				if (verb === "list") {
					const sources = readSources();
					const names = Object.keys(sources);
					if (names.length === 0) {
						return { text: "No marketplaces configured.\n\nGet started: /marketplace add <path|url>" };
					}
					return {
						text: names.map(name => `  ${name}  ${sources[name].location}`).join("\n"),
					};
				}
				if (verb === "update") {
					const sources = readSources();
					const names = tokens.length > 1 ? [tokens[1]] : Object.keys(sources);
					if (names.length === 0) return { text: "No marketplaces configured." };
					const lines = [];
					for (const name of names) {
						const source = sources[name];
						if (!source) return { text: `Marketplace "${name}" is not configured.` };
						const count = await updateSource(name, source.location);
						lines.push(`  ${name}: ${count} plugins`);
					}
					return { text: `Updated:\n${lines.join("\n")}` };
				}
				if (verb === "discover") {
					const sources = readSources();
					const names = tokens.length > 1 ? [tokens[1]] : Object.keys(sources);
					const lines = [];
					for (const name of names) {
						const cache = readJson(join(marketplaceCache, `${name}.json`), null);
						if (!Array.isArray(cache)) {
							lines.push(`  ${name}: not updated; run /marketplace update ${name}`);
							continue;
						}
						for (const plugin of cache) {
							lines.push(`  ${plugin.name} | ${plugin.version || "-"} | ${plugin.description || "-"} [${name}]`);
						}
					}
					return { text: lines.length === 0 ? "No plugins discovered. Run /marketplace update first." : lines.join("\n") };
				}
				if (verb === "installed") {
					const document = readProfilePackage();
					const dependencies = document.dependencies ?? {};
					const lines = Object.entries(dependencies).map(([name, version]) => `  ${name} ${version}`);
					return { text: lines.length === 0 ? "No plugins installed." : `Installed plugins:\n${lines.join("\n")}` };
				}
				if (verb === "install" || verb === "uninstall" || verb === "upgrade") {
					if (verb === "upgrade") {
						await runNpm(["update", "--prefix", profileDir, "--no-audit", "--no-fund"]);
						return { text: "Profile dependencies updated; run /reload-plugins." };
					}
					const target = tokens[1];
					if (!target) return { text: `Usage: /marketplace ${verb} <name@source>` };
					const at = target.lastIndexOf("@");
					if (at <= 0) return { text: `Usage: /marketplace ${verb} <name@source>` };
					const pluginName = target.slice(0, at);
					const sourceNameValue = target.slice(at + 1);
					const source = readSources()[sourceNameValue];
					if (!source) return { text: `Marketplace "${sourceNameValue}" is not configured.` };
					const plugin = await cachedPlugin(sourceNameValue, pluginName);
					if (verb === "uninstall") {
						await runNpm(["uninstall", "--prefix", profileDir, "--no-audit", "--no-fund", plugin.package]);
						removeBundle(plugin.bundle || plugin.package);
						return { text: `Uninstalled ${plugin.package}; run /reload-plugins.` };
					}
					const spec = plugin.version.length > 0 ? `${plugin.package}@${plugin.version}` : plugin.package;
					await runNpm(["install", "--prefix", profileDir, "--no-audit", "--no-fund", spec]);
					addBundle(plugin.bundle);
					return { text: `Installed ${spec}; run /reload-plugins.` };
				}
				return { text: marketplaceHelp };
			} catch (error) {
				fail("marketplace/error", messageOf(error));
			}
		},

		async "cleanse"(params) {
			const agent = liveAgent(params.sessionId);
			if (agent.status === "running") fail("cleanse/busy", "wait for the current turn to finish");
			const raw = typeof params.request === "string" ? params.request.trim() : "";
			const all = raw.split(/\s+/).includes("--all");
			const request = raw.replace(/(^|\s)--all(?=\s|$)/g, " ").trim();
			const scope = request.length === 0 ? "all project diagnostics" : request;
			const controller = new AbortController();
			const activation = await startSideActivation(agent, [{
				type: "text",
				text: [
					"You are the project diagnostics repair worker. Work autonomously in the parent workspace.",
					`Scope: ${scope}.`,
					"Discover concrete diagnostics from the repository's own build, test, typecheck, and lint commands.",
					"Fix the failures with the smallest coherent edits; do not touch unrelated files or rewrite the project.",
					"Rerun the same checks after fixing, and stop when they pass or the remaining failure needs a human decision.",
					"Finish with a concise report: Diagnostics, Files changed, Verification, Remaining.",
					all ? "Cover every diagnostic you can find, not just the first failure." : "",
				].filter(line => line.length > 0).join("\n"),
			}], "cleanse", controller.signal);
			try {
				const result = await activation.result;
				const output = (result.output ?? [])
					.filter(block => block?.type === "text" && typeof block.text === "string")
					.map(block => block.text)
					.join("");
				return {
					childId: activation.childId,
					text: output.length === 0 ? "Cleanse finished without a report." : output,
					stopReason: result.stopReason,
					diagnostic: result.diagnostic ?? "",
				};
			} finally {
				await activation.dispose();
			}
		},

		async "browser"(params) {
			return featureCommand("browser", typeof params.args === "string" ? params.args : "");
		},
		async "computer"(params) {
			return featureCommand("computer", typeof params.args === "string" ? params.args : "");
		},

		async "login"(params) {
			const args = typeof params.args === "string" ? params.args.trim() : "";
			if (args.length > 0) {
				if (/^https?:\/\//i.test(args)) {
					return { text: "Manual OAuth redirect URLs are unavailable; the local callback finishes the DeepSeek sign-in." };
				}
				if (args.toLowerCase() !== "deepseek") {
					return { text: `OAuth provider "${args}" is unavailable; this profile signs in to DeepSeek.` };
				}
			}
			const account = ctx.get("deepseekAccount");
			const webServer = ctx.get("webServer");
			if (account === undefined || webServer === undefined) {
				fail("account/unavailable", "this profile mounts no DeepSeek account or web server");
			}
			const client = {
				version: "booth",
				locale: "en",
				timezoneOffsetSeconds: -new Date().getTimezoneOffset() * 60,
			};
			let state = await account.getState();
			if (state.status === "credential-stored") {
				return { text: "Already signed in to the DeepSeek account." };
			}
			state = await account.startSignIn(client, webServer.getOrigin(), "web");
			for (let i = 0; i < 150; i += 1) {
				const attempt = state.attempt;
				if (attempt?.authorizeUrl) {
					return { url: attempt.authorizeUrl, text: "Open this URL to sign in; the local callback will finish the flow." };
				}
				if (attempt && ["failed", "expired", "cancelled"].includes(attempt.phase)) {
					fail("account/sign-in", `sign-in ${attempt.phase}${attempt.errorCode ? `: ${attempt.errorCode}` : ""}`);
				}
				await new Promise(resolve => setTimeout(resolve, 100));
				state = await account.getState();
			}
			return { text: "Sign-in started; no browser URL arrived within 15 seconds." };
		},

		async "logout"(params) {
			const args = typeof params.args === "string" ? params.args.trim() : "";
			if (args.length > 0 && args.toLowerCase() !== "deepseek") {
				return { text: `OAuth provider "${args}" is unavailable; this profile signs out of DeepSeek.` };
			}
			const account = ctx.get("deepseekAccount");
			if (account === undefined) fail("account/unavailable", "this profile mounts no DeepSeek account");
			const client = {
				version: "booth",
				locale: "en",
				timezoneOffsetSeconds: -new Date().getTimezoneOffset() * 60,
			};
			const state = await account.getState();
			if (state.status === "signed-out") return { text: "Already signed out." };
			await account.signOut(client);
			return { text: "Signed out of the DeepSeek account." };
		},

		async "vibe"(params) {
			const agent = liveAgent(params.sessionId);
			const args = typeof params.args === "string" ? params.args.trim() : "";
			const verb = args.toLowerCase();
			if (verb === "status" || verb === "") return { text: `Vibe mode: ${vibeEnabled.get(agent.id) === true ? "on" : "off"}.` };
			if (verb === "on" || verb === "off") {
				vibeEnabled.set(agent.id, verb === "on");
				return { text: `Vibe mode ${verb}.` };
			}
			const controller = new AbortController();
			const activation = await startSideActivation(agent, [{
				type: "text",
				text: [
					"You are a read-only vibe worker. Inspect and answer with read and search tools only.",
					"Do not edit files, run destructive commands, or change the parent conversation.",
					args,
				].join("\n"),
			}], "vibe", controller.signal);
			try {
				const result = await activation.result;
				const output = (result.output ?? [])
					.filter(block => block?.type === "text" && typeof block.text === "string")
					.map(block => block.text)
					.join("")
					.trim();
				return { text: output.length === 0 ? "Vibe worker returned no text." : output };
			} finally {
				await activation.dispose();
			}
		},

		async "providers.get"(params) {
			return providerView(String(params.provider ?? ""));
		},

		async "providers.list"(params) {
			const descriptor = providerDescriptor();
			const providers = descriptor.value && typeof descriptor.value === "object" ? descriptor.value.providers : {};
			const names = new Set(Object.keys(providers ?? {}));
			for (const row of providerDirectory()) names.add(row.provider);
			const rows = [];
			for (const provider of names) rows.push(await providerView(provider));
			rows.sort((left, right) => left.provider.localeCompare(right.provider));
			return { providers: rows };
		},

		async "providers.update"(params) {
			const provider = String(params.provider ?? "").trim();
			if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(provider)) fail("providers/bad-name", "provider name must use letters, digits, dot, underscore or hyphen");
			const descriptor = providerDescriptor();
			const profile = providerProfile(descriptor, provider);
			const ops = [];
			const setOrUnset = (field, value) => {
				if (value === undefined) return;
				if (value === "") ops.push({ op: "unset", path: ["providers", provider, field] });
				else ops.push({ op: "set", path: ["providers", provider, field], value });
			};
			if (typeof params.baseURL === "string") {
				const value = params.baseURL.trim();
				if (value.length > 0) {
					let parsed;
					try {
						parsed = new URL(value);
					} catch {
						fail("providers/bad-url", "baseURL must be a valid http(s) URL");
					}
					if (parsed.protocol !== "http:" && parsed.protocol !== "https:") fail("providers/bad-url", "baseURL must use http or https");
				}
				setOrUnset("baseURL", value);
			}
			if (typeof params.api === "string") setOrUnset("api", params.api.trim());
			if (typeof params.displayName === "string") setOrUnset("displayName", params.displayName.trim());
			if (Array.isArray(params.models)) {
				const existingModels = new Map();
				if (Array.isArray(profile?.models)) {
					for (const model of profile.models) {
						const id = typeof model === "string" ? model : model?.id;
						if (typeof id === "string" && id.length > 0) existingModels.set(id, model);
					}
				}
				const models = [];
				const seen = new Set();
				for (const entry of params.models) {
					const id = typeof entry === "string" ? entry.trim() : typeof entry?.id === "string" ? entry.id.trim() : "";
					if (id.length === 0 || seen.has(id)) continue;
					seen.add(id);
					const existing = existingModels.get(id);
					if (typeof entry === "object" && entry !== null) models.push({ ...entry, id });
					else if (existing !== undefined) models.push({ ...existing, id });
					else models.push({ id });
				}
				if (models.length === 0) ops.push({ op: "unset", path: ["providers", provider, "models"] });
				else ops.push({ op: "set", path: ["providers", provider, "models"], value: models });
			}
			let keyRef;
			if (typeof params.apiKey === "string" && params.apiKey.trim().length > 0) {
				const key = params.apiKey.trim();
				if (/[\s\x00-\x1f]/.test(key)) fail("providers/bad-key", "API key must not contain whitespace or control characters");
				keyRef = typeof profile?.apiKeyEnv === "string" && profile.apiKeyEnv.length > 0 ? profile.apiKeyEnv : apiKeyRef(provider);
				if (profile?.apiKeyEnv === undefined) ops.push({ op: "set", path: ["providers", provider, "apiKeyEnv"], value: keyRef });
			}
			let keyCleared = false;
			if (params.removeKey === true) {
				const ref = typeof profile?.apiKeyEnv === "string" && profile.apiKeyEnv.length > 0 ? profile.apiKeyEnv : apiKeyRef(provider);
				await ctx.credentials.unset(ref);
				keyCleared = true;
			}
			if (ops.length > 0) await ctx.settings.mutate("llm-pi-ai", ops, descriptor.revision);
			if (keyRef !== undefined) await ctx.credentials.set(keyRef, params.apiKey.trim());
			if (ops.length === 0 && keyRef === undefined && !keyCleared) {
				return { ...(await providerView(provider)), text: `No changes for ${provider}.` };
			}
			return { ...(await providerView(provider)), text: `${keyCleared ? "Cleared key for" : "Updated"} ${provider}.` };
		},

		async "providers.discover"(params) {
			const provider = String(params.provider ?? "").trim();
			if (provider.length === 0) fail("providers/bad-request", "provider is required");
			const request = { provider };
			if (typeof params.baseURL === "string" && params.baseURL.trim().length > 0) request.baseURL = params.baseURL.trim();
			if (typeof params.api === "string" && params.api.trim().length > 0) request.api = params.api.trim();
			if (typeof params.apiKey === "string" && params.apiKey.trim().length > 0) request.apiKey = params.apiKey.trim();
			if (typeof ctx.llm?.discoverModels !== "function") fail("providers/discover", "this profile mounts no model discovery");
			let models;
			try {
				models = await ctx.llm.discoverModels("llm-pi-ai", request);
			} catch (error) {
				fail("providers/discover", messageOf(error));
			}
			const rows = models.map(model => {
				const context = Number(model.contextWindow) > 0 ? ` · ${Math.round(Number(model.contextWindow) / 1000)}k ctx` : "";
				const name = typeof model.name === "string" && model.name.length > 0 && model.name !== model.id ? ` — ${model.name}` : "";
				return `  ${model.id}${name}${context}`;
			});
			const shown = rows.slice(0, 30);
			if (rows.length > shown.length) shown.push(`  … +${rows.length - shown.length} more`);
			return {
				models: models.map(model => ({ id: model.id, name: model.name ?? "", contextWindow: model.contextWindow })),
				text: `models for ${provider} (${models.length})${models.length === 0 ? "\n  (none)" : "\n" + shown.join("\n")}`,
			};
		},

		async "providers.remove"(params) {
			const provider = String(params.provider ?? "").trim();
			const descriptor = providerDescriptor();
			const profile = providerUserProfile(descriptor, provider);
			if (profile === undefined) return { text: `${provider} has no user-level configuration to remove.` };
			const ref = typeof profile.apiKeyEnv === "string" && profile.apiKeyEnv.length > 0 ? profile.apiKeyEnv : apiKeyRef(provider);
			await ctx.settings.mutate("llm-pi-ai", [{ op: "unset", path: ["providers", provider] }], descriptor.revision);
			let removedKey = false;
			if (params.removeKey === true) {
				try {
					await ctx.credentials.unset(ref);
					removedKey = true;
				} catch (error) {
					return { text: `Removed ${provider}, but could not delete ${ref}: ${messageOf(error)}` };
				}
			}
			return { text: `Removed user configuration for ${provider}${removedKey ? ` and key ${ref}` : ""}; profile defaults remain.` };
		},

		async "fast"(params) {
			const agent = liveAgent(params.sessionId);
			const verb = typeof params.args === "string" ? params.args.trim().toLowerCase() : "";
			if (verb === "status" || verb === "") return { text: `Fast mode: ${serviceTiers.get(agent.id) === "priority" ? "on" : "off"}.` };
			if (verb === "off") {
				setServiceTier(agent, undefined);
				return { text: "Fast mode disabled." };
			}
			if (verb === "on" || verb === "ultra") {
				setServiceTier(agent, "priority");
				return { text: "Fast mode enabled (service_tier=priority)." };
			}
			return { text: "Usage: /fast [on|ultra|off|status]" };
		},

		async "slow"(params) {
			const agent = liveAgent(params.sessionId);
			const verb = typeof params.args === "string" ? params.args.trim().toLowerCase() : "";
			if (verb === "status" || verb === "") return { text: `Slow mode: ${serviceTiers.get(agent.id) === "flex" ? "on" : "off"}.` };
			if (verb === "off") {
				setServiceTier(agent, undefined);
				return { text: "Slow mode disabled." };
			}
			if (verb === "on") {
				setServiceTier(agent, "flex");
				return { text: "Slow mode enabled (service_tier=flex)." };
			}
			return { text: "Usage: /slow [on|off|status]" };
		},

		async "share"(params) {
			const agent = liveAgent(params.sessionId);
			const existing = shares.get(agent.id);
			if (existing !== undefined) return { url: existing.url, text: `Already sharing: ${existing.url}` };
			const tier = typeof params.args === "string" ? params.args.trim().toLowerCase() : "";
			const share = await startShare(agent, tier === "write");
			return { url: share.url, text: `Sharing this session at ${share.url} (${share.write ? "write" : "read-only"}).` };
		},

		async "collab"(params) {
			const agent = liveAgent(params.sessionId);
			const args = typeof params.args === "string" ? params.args.trim().toLowerCase() : "";
			const verb = args.length === 0 ? "status" : args.split(/\s+/)[0];
			if (verb === "list") {
				const rows = [...shares.values()].map(share => {
					const title = ctx.get("sessionTitle")?.get(share.agent.session)?.title ?? "";
					const name = title.length > 0 ? title : share.agent.id;
					return `  ${name} (${share.agent.id}) — ${share.write ? "control" : "view-only"} — ${share.url}`;
				});
				return { text: rows.length === 0 ? "No active Collab hosts." : `Active Collab hosts:\n${rows.join("\n")}` };
			}
			if (verb === "stop") {
				return { text: (await stopShare(agent)) ? "Collab stopped." : "No active share." };
			}
			if (verb === "status") {
				const share = shares.get(agent.id);
				if (share === undefined) return { text: "Not sharing this session. Use /collab start or /collab view." };
				return { text: `Collab: ${share.write ? "control" : "view-only"} — ${share.url}` };
			}
			if (verb === "start" || verb === "view" || verb === "on" || verb === "off") {
				const write = verb === "start" || verb === "on";
				let share = shares.get(agent.id);
				if (share === undefined) share = await startShare(agent, write);
				else share.write = write;
				return { text: `Collab: ${write ? "control" : "view-only"} — ${share.url}` };
			}
			return { text: "Usage: /collab [start|view|list|stop|status] [relayUrl]" };
		},

		async "join"(params) {
			const agent = liveAgent(params.sessionId);
			const requested = typeof params.args === "string" ? params.args.trim() : "";
			const share = shares.get(agent.id);
			const url = requested.length === 0 ? share?.url : requested;
			if (url === undefined) return { text: "No active share to join; pass a share URL or run /share first." };
			let parsed;
			try {
				parsed = new URL(url);
			} catch {
				return { text: "Invalid share URL." };
			}
			if (parsed.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]", "::1"].includes(parsed.hostname)) {
				return { text: "Only local share URLs are supported." };
			}
			return { url, text: `Joining ${url}` };
		},

		async "leave"(params) {
			const agent = liveAgent(params.sessionId);
			return { text: (await stopShare(agent)) ? "Stopped sharing this session." : "No active share." };
		},

		async "extended-context"(params) {
			const agent = liveAgent(params.sessionId);
			const args = typeof params.args === "string" ? params.args.trim().toLowerCase() : "";
			const verb = args.length === 0 ? "toggle" : args.split(/\s+/)[0];
			if (verb === "status") {
				const active = extendedPrevious.get(agent.id);
				const current = agent.model.current?.();
				return {
					text: active === undefined
						? `Extended context: off. Current model: ${current ? `${current.provider}/${current.model}` : "unknown"}.`
						: `Extended context: on. Current model: ${current ? `${current.provider}/${current.model}` : "unknown"} (${active.contextWindow ?? "?"} tokens); previous: ${active.previous ? `${active.previous.provider}/${active.previous.model}` : "unknown"}.`,
				};
			}
			if (verb === "off" || (verb === "toggle" && extendedPrevious.has(agent.id))) {
				const active = extendedPrevious.get(agent.id);
				extendedPrevious.delete(agent.id);
				if (active?.previous !== undefined) agent.model.select(active.previous);
				return { text: "Extended context disabled; restored the previous model." };
			}
			if (verb !== "on" && verb !== "toggle") return { text: "Usage: /extended-context [on|off|status]" };
			if (extendedPrevious.has(agent.id)) return { text: "Extended context is already on." };
			const previous = agent.model.current?.();
			let best;
			for (const provider of ctx.llm.listProviders()) {
				let models = [];
				try {
					models = await ctx.llm.listModels(provider.id);
				} catch {
					continue;
				}
				for (const model of models) {
					try {
						const info = await ctx.llm.resolveModelInfo(provider.id, model.id);
						const contextWindow = info.context?.contextWindow;
						if (contextWindow !== undefined && (best === undefined || contextWindow > best.contextWindow)) {
							best = { provider: provider.id, model: model.id, label: `${provider.id}/${model.id}`, contextWindow };
						}
					} catch {
						// A model without resolvable context cannot participate in this comparison.
					}
				}
			}
			if (best === undefined) return { text: "No model with a known context window is available." };
			agent.model.select({ provider: best.provider, model: best.model });
			extendedPrevious.set(agent.id, { previous, contextWindow: best.contextWindow });
			return { text: `Extended context on: using ${best.label} (${best.contextWindow} tokens).` };
		},

		async "skillful"(params) {
			const agent = liveAgent(params.sessionId);
			const args = typeof params.args === "string" ? params.args.trim().toLowerCase() : "";
			const verb = args.length === 0 ? "toggle" : args.split(/\s+/)[0];
			if (verb === "status") {
				return {
					text: `Skill listing: ${skillfulOff.get(agent.id) === true ? "off" : "on"} (session override; dsh skill tool hidden while off).`,
				};
			}
			if (verb === "off") {
				setSkillful(agent, false);
				return { text: "Skill listing disabled for this session." };
			}
			if (verb === "on") {
				setSkillful(agent, true);
				return { text: "Skill listing enabled for this session." };
			}
			if (verb === "toggle") {
				const next = skillfulOff.get(agent.id) !== true;
				setSkillful(agent, next);
				return { text: `Skill listing ${next ? "enabled" : "disabled"} for this session.` };
			}
			return { text: "Usage: /skillful [on|off|status]" };
		},

		async "force"(params) {
			const agent = liveAgent(params.sessionId);
			const args = typeof params.args === "string" ? params.args.trim() : "";
			const tokens = splitArgs(args);
			const verb = tokens[0] ?? "status";
			if (verb === "status") return { text: forceStatus(agent) };
			if (verb === "off") {
				if (forceRuns.has(agent.id)) return { text: "Force is active for this turn and cannot be cancelled mid-turn." };
				const armed = forceArmed.delete(agent.id);
				return { text: armed ? "Force off: canceled the pending restriction." : "Force already off." };
			}
			if (tokens.length > 1) return { text: "Usage: /force <tool-name> [prompt]" };
			const name = tokens[0];
			const names = ctx.tools.definitions(agent).map(definition => definition.name);
			if (!names.includes(name)) return { text: `Tool "${name}" is not available in this session.` };
			forceArmed.set(agent.id, { tool: name });
			return { text: `Force armed: only "${name}" will be visible at the next user turn.` };
		},

		async "plan-review"(params) {
			const agent = liveAgent(params.sessionId);
			const plan = ctx.get("planMode")?.get(agent);
			if (plan?.active !== true) return { text: "Plan mode is inactive; /plan-review needs an active plan." };
			const transcript = await loadTurnText(agent);
			const controller = new AbortController();
			const activation = await startSideActivation(agent, [{
				type: "text",
				text: [
					"You are reviewing the active implementation plan. Do not modify files.",
					"Find gaps, unsafe assumptions, missing verification, and unclear steps.",
					"Return 3 to 6 terse bullets; end with the single most important plan correction.",
					"",
					transcript,
				].join("\n"),
			}], "plan-review", controller.signal);
			try {
				const result = await activation.result;
				const review = (result.output ?? [])
					.filter(block => block?.type === "text" && typeof block.text === "string")
					.map(block => block.text)
					.join("")
					.trim();
				return { text: review.length === 0 ? "Plan review returned no text." : review };
			} finally {
				await activation.dispose();
			}
		},

		async "advisor"(params) {
			const agent = liveAgent(params.sessionId);
			const args = typeof params.args === "string" ? params.args.trim().toLowerCase() : "";
			const verb = args.length === 0 ? "toggle" : args.split(/\s+/)[0];
			if (verb === "on" || verb === "toggle") {
				const next = verb === "toggle" ? !advisorEnabled.get(agent.id) : true;
				advisorEnabled.set(agent.id, next);
				return { text: next ? "Advisor enabled." : "Advisor disabled." };
			}
			if (verb === "off") {
				advisorEnabled.set(agent.id, false);
				return { text: "Advisor disabled." };
			}
			if (verb === "status") {
				const notes = advisorNotes.get(agent.id) ?? [];
				return {
					text: `Advisor: ${advisorEnabled.get(agent.id) === true ? "on" : "off"}\nnotes: ${notes.length}\nrunning: ${advisorRuns.has(agent.id) ? "yes" : "no"}`,
				};
			}
			if (verb === "dump") {
				const notes = advisorNotes.get(agent.id) ?? [];
				return { text: notes.length === 0 ? "Advisor has no notes for this session." : notes.join("\n\n") };
			}
			if (verb === "configure") {
				return { text: "/advisor configure is unavailable; the advisor uses a review subagent." };
			}
			return { text: "Usage: /advisor [on|off|status|dump|configure]" };
		},

		async "omfg"(params) {
			const agent = liveAgent(params.sessionId);
			const complaint = typeof params.complaint === "string" ? params.complaint.trim() : "";
			if (complaint.length === 0) return { text: "Usage: /omfg <complaint>" };
			const controller = new AbortController();
			const activation = await startSideActivation(agent, [{
				type: "text",
				text: [
					"You convert a user complaint into one durable behavioral rule for future turns.",
					"Read the complaint, identify the recurring behavior to stop, and output only a terse imperative rule (one or two sentences).",
					"Do not explain, quote the complaint, or use markdown fences.",
					`Complaint: ${complaint}`,
				].join("\n"),
			}], "omfg", controller.signal);
			try {
				const result = await activation.result;
				const rule = (result.output ?? [])
					.filter(block => block?.type === "text" && typeof block.text === "string")
					.map(block => block.text)
					.join("")
					.trim();
				if (rule.length === 0) return { text: "The rule forge returned no text." };
				const current = readRules().trimEnd();
				writeRules(`${current.length === 0 ? "" : `${current}\n`}- ${rule.replace(/^-\s*/, "")}\n`);
				return { text: `Rule added: ${rule}` };
			} finally {
				await activation.dispose();
			}
		},

		async "prewalk"(params) {
			const agent = liveAgent(params.sessionId);
			const args = typeof params.args === "string" ? params.args.trim() : "";
			const tokens = splitArgs(args);
			const verb = tokens[0] ?? "status";
			if (verb === "status") return { text: prewalkStatus(agent) };
			if (verb === "off") {
				if (prewalkSwitches.has(agent.id)) return { text: "Prewalk is active for this turn and cannot be cancelled mid-turn." };
				const armed = prewalkArmed.delete(agent.id);
				return { text: armed ? "Prewalk off: canceled the pending handoff; keeping the active model." : "Prewalk already off for this session; keeping the active model." };
			}
			if (verb === "restart") {
				if (tokens.length > 3) return { text: "Usage: /prewalk [into] | restart [from] [into] | off" };
				if (tokens.length < 3) return { text: "Usage: /prewalk restart [from] [into]" };
				const from = await resolvePrewalkTarget(tokens[1]);
				agent.model.select({ provider: from.provider, model: from.model });
				const into = await resolvePrewalkTarget(tokens[2]);
				prewalkArmed.set(agent.id, into);
				return { text: `Prewalk restarted: using ${from.label} for planning, then ${into.label} at the next user turn.` };
			}
			if (verb === "status" || tokens.length === 0) return { text: prewalkStatus(agent) };
			if (tokens.length > 1) return { text: "Usage: /prewalk [into] | restart [from] [into] | off" };
			const target = await resolvePrewalkTarget(tokens[0]);
			prewalkArmed.set(agent.id, target);
			return { text: `Prewalk on: switching to ${target.label} at the next user turn.` };
		},

		async "security"(params) {
			const agent = liveAgent(params.sessionId);
			const args = typeof params.args === "string" ? params.args.trim() : "";
			const tokens = splitArgs(args);
			const verb = tokens[0] ?? "status";
			if (verb === "help") return { text: securityHelp };
			if (verb === "scan") {
				if (agent.status === "running") fail("security/busy", "wait for the current turn to finish");
				const raw = tokens.slice(1).join(" ").trim();
				const all = raw.split(/\s+/).includes("--all");
				const request = raw.replace(/(^|\s)--all(?=\s|$)/g, " ").trim() || "all security issues";
				const controller = new AbortController();
				const id = `scan-${Date.now().toString(36)}-${randomUUID().slice(0, 6)}`;
				const activation = await startSideActivation(agent, [{
					type: "text",
					text: [
						"You are the security audit worker. Audit the parent workspace without modifying files.",
						`Scope: ${request}.`,
						"Inspect source, configuration, dependencies, secrets, authentication boundaries, and dangerous data flows.",
						"For each finding report: severity, file and line, evidence, impact, and a concrete fix.",
						"Do not execute destructive commands and do not remediate unless the user explicitly asked for a fix.",
						all ? "Cover the entire repository, including lower-severity issues and dependency risks." : "",
						"Finish with a concise report: Summary, Findings, Open questions.",
					].filter(line => line.length > 0).join("\n"),
				}], "security", controller.signal);
				const started = new Date().toISOString();
				const index = readSecurityIndex();
				index.scans[id] = { id, request, status: "running", started, childId: activation.childId };
				writeSecurityIndex(index);
				securityRuns.set(id, { controller, activation });
				const reportPath = join(securityDir, `${id}.md`);
				void (async () => {
					try {
						const result = await activation.result;
						const report = securityText(result);
						mkdirSync(securityDir, { recursive: true, mode: 0o700 });
						writeFileSync(reportPath, report, { mode: 0o600 });
						const current = readSecurityIndex();
						if (current.scans[id]?.status === "running") {
							current.scans[id] = { ...current.scans[id], status: "complete", finished: new Date().toISOString(), report: reportPath };
							writeSecurityIndex(current);
						}
					} catch (error) {
						const current = readSecurityIndex();
						if (current.scans[id]?.status === "running") {
							current.scans[id] = { ...current.scans[id], status: "failed", finished: new Date().toISOString(), diagnostic: messageOf(error) };
							writeSecurityIndex(current);
						}
					} finally {
						await activation.dispose();
						securityRuns.delete(id);
					}
				})();
				return { text: `Security scan started: ${id}`, id };
			}
			if (verb === "status") {
				const index = readSecurityIndex();
				const id = tokens[1];
				if (id) {
					const scan = index.scans[id];
					if (!scan) return { text: `Scan "${id}" not found.` };
					return { text: `scan ${id}: ${scan.status}\nrequest: ${scan.request}\nstarted: ${scan.started}${scan.finished ? `\nfinished: ${scan.finished}` : ""}` };
				}
				const running = Object.values(index.scans).filter(scan => scan.status === "running");
				return { text: running.length === 0 ? "No security scans running." : `Running scans: ${running.map(scan => scan.id).join(", ")}` };
			}
			if (verb === "scans") {
				const scans = Object.values(readSecurityIndex().scans)
					.sort((a, b) => b.started.localeCompare(a.started));
				if (scans.length === 0) return { text: "No security scans stored." };
				return {
					text: scans.map(scan => `${scan.id} | ${scan.status} | ${scan.started} | ${scan.request}`).join("\n"),
				};
			}
			if (verb === "show") {
				const id = tokens[1];
				if (!id) return { text: "Usage: /security show <id>" };
				const scan = readSecurityIndex().scans[id];
				if (!scan) return { text: `Scan "${id}" not found.` };
				if (scan.status === "running") return { text: `Scan "${id}" is still running.` };
				if (scan.report) {
					try {
						return { text: readFileSync(scan.report, "utf8") };
					} catch (error) {
						if (error?.code !== "ENOENT") throw error;
					}
				}
				return { text: `Scan "${id}" has no report (${scan.status}).` };
			}
			if (verb === "cancel") {
				const id = tokens[1] ?? [...securityRuns.keys()][0];
				if (!id) return { text: "No security scan is running." };
				const run = securityRuns.get(id);
				if (!run) return { text: `Scan "${id}" is not running.` };
				run.controller.abort();
				const index = readSecurityIndex();
				if (index.scans[id]) {
					index.scans[id] = { ...index.scans[id], status: "cancelled", finished: new Date().toISOString() };
					writeSecurityIndex(index);
				}
				await run.activation.dispose();
				securityRuns.delete(id);
				return { text: `Cancelled security scan ${id}.` };
			}
			if (verb === "plan") {
				const request = tokens.slice(1).join(" ").trim() || "the repository";
				return { text: `Security scan plan:\n  target: ${request}\n  method: read-only audit subagent\n  output: stored Markdown report\nStart with /security scan ${request === "the repository" ? "" : request}`.trim() };
			}
			if (["import", "export", "validate", "compare", "disposition"].includes(verb)) {
				return { text: `security ${verb} is unavailable in the booth security backend.` };
			}
			return { text: securityHelp };
		},

		async "side.ask"(params) {
			const agent = liveAgent(params.sessionId);
			const question = typeof params.text === "string" ? params.text.trim() : "";
			if (question.length === 0) fail("control/bad-request", "a question is required");
			const controller = new AbortController();
			const activation = await startSideActivation(agent, [{
					type: "text",
					text: `Answer this side question for the user. Do not edit files, do not change the parent conversation, and keep the answer focused.\n\n${question}`,
				}], "btw", controller.signal);
			try {
				const result = await activation.result;
				const output = (result.output ?? [])
					.filter(block => block?.type === "text" && typeof block.text === "string")
					.map(block => block.text)
					.join("");
				return {
					childId: activation.childId,
					output,
					stopReason: result.stopReason,
					diagnostic: result.diagnostic ?? "",
				};
			} finally {
				await activation.dispose();
			}
		},

		async "side.background"(params) {
			const agent = liveAgent(params.sessionId);
			const work = typeof params.text === "string" ? params.text.trim() : "";
			if (work.length === 0) fail("control/bad-request", "work is required");
			const controller = new AbortController();
			const activation = await startSideActivation(agent, [{
					type: "text",
					text: `Work on this tangential task autonomously. Use tools as needed, stay inside the parent workspace, and finish with a concise report.\n\n${work}`,
				}], "tan", controller.signal);
			// Own the activation until it settles; the child keeps running after
			// this request returns, and its result is not injected into the parent.
			void activation.result.then(() => activation.dispose(), () => activation.dispose());
			return { childId: activation.childId };
		},

		async "todos.commit"(params) {
			const agent = liveAgent(params.sessionId);
			const input = params.todos;
			if (!Array.isArray(input)) fail("todo/bad-request", "todos must be an array");
			if (input.length > 500) fail("todo/bad-request", "todo list is limited to 500 items");
			const todos = input.map((item, index) => {
				const content = typeof item?.content === "string" ? item.content.trim() : "";
				if (content.length === 0) fail("todo/bad-request", `todos[${index}].content must be non-empty`);
				const status = item?.status;
				if (status !== "pending" && status !== "in_progress" && status !== "completed") {
					fail("todo/bad-request", `todos[${index}].status must be pending, in_progress or completed`);
				}
				return { content, status };
			});
			agent.session.append("todo/write", { todos });
			return { count: todos.length };
		},

		async "session.shake"(params) {
			const agent = liveAgent(params.sessionId);
			if (agent.busy) fail("shake/busy", "wait for the current turn to finish before eliding");
			const mode = typeof params.mode === "string" && params.mode.length > 0 ? params.mode : "elide";
			if (mode !== "elide") {
				fail("shake/unsupported", `this harness can only elide tool results; "${mode}" needs a model rewrite pass`);
			}
			const result = requireService("toolResultPruner").pruneSession(agent.session);
			return { pruned: result.pruned.length, charsRemoved: result.charsRemoved };
		},

		async "session.info"(params) {
			const agent = liveAgent(params.sessionId);
			const titles = ctx.get("sessionTitle");
			const goal = ctx.get("goals")?.get(agent);
			const plan = ctx.get("planMode")?.get(agent);
			return {
				sessionId: agent.id,
				cwd: agent.session.header.cwd ?? "",
				title: titles?.get(agent.session)?.title ?? "",
				status: agent.status,
				goal:
					goal === undefined
						? null
						: {
								id: goal.id,
								revision: goal.revision,
								objective: goal.objective,
								phase: goal.phase,
								activation: goal.activation,
								roundsStarted: goal.roundsStarted,
								maxGoalRounds: goal.maxGoalRounds,
								blockedReason: goal.blockedReason ?? null,
							},
				plan: plan ?? { active: false },
			};
		},

		async "tools.list"(params) {
			const agent = liveAgent(params.sessionId);
			const tools = requireService("tools");
			const list =
				typeof tools.definitions === "function"
					? tools.definitions(agent)
					: typeof tools.schemas === "function"
						? tools.schemas(agent)
						: typeof tools.view === "function"
							? [...(tools.view(agent)?.visible?.values?.() ?? [])]
							: [];
			return [...list].map(definition => ({
				name: definition.name,
				description: definition.description ?? "",
			}));
		},

		async "agents.list"(params) {
			const titles = ctx.get("sessionTitle");
			const rows = ctx.agents.list().map(agent => ({
				sessionId: agent.id,
				cwd: agent.session.header.cwd ?? "",
				title: titles?.get(agent.session)?.title ?? "",
				status: agent.status,
				busy: agent.busy,
				finished: false,
			}));
			const parent = typeof params?.sessionId === "string" ? params.sessionId : "";
			if (parent) {
				const live = new Set(rows.map(row => row.sessionId));
				try {
					const records = await ctx.sessionQuery.listSessions();
					for (const record of records) {
						const header = record.header;
						if (header.parentSession !== parent || live.has(header.id)) continue;
						rows.push({
							sessionId: header.id,
							cwd: header.cwd ?? "",
							title: "",
							status: "finished",
							busy: false,
							finished: true,
						});
					}
				} catch (error) {
					ctx.logger?.warn?.(`booth-control: finished agents: ${messageOf(error)}`);
				}
			}
			return rows;
		},

		async "jobs.list"(params) {
			const jobs = requireService("jobs");
			const owner = typeof params.sessionId === "string" ? params.sessionId : undefined;
			return jobs.list(owner).map(job => ({
				id: job.id,
				kind: job.kind,
				label: job.label,
				status: job.status,
				progress: job.progress ?? "",
				detail: job.detail ?? "",
				startedAt: job.startedAt,
				finishedAt: job.finishedAt ?? null,
				output: job.output,
			}));
		},

		async "jobs.kill"(params) {
			const jobs = requireService("jobs");
			const owner = typeof params.sessionId === "string" ? params.sessionId : undefined;
			if (params.id === "all") {
				const ids = jobs.list(owner).filter(job => job.status === "running").map(job => job.id);
				for (const id of ids) jobs.kill(id, owner, "killed from booth");
				return { killed: ids, text: ids.length === 0 ? "no running jobs to kill" : `killed ${ids.length} job(s)` };
			}
			if (typeof params.id !== "string" || params.id.length === 0) {
				fail("control/bad-request", "job id is required");
			}
			const result = jobs.kill(params.id, owner, "killed from booth");
			const outcome = result?.outcome ?? result;
			const text = outcome === "already-finished" ? `job ${params.id} already finished` : `kill requested for ${params.id}`;
			return { killed: [params.id], result, text };
		},

		async "plan.status"(params) {
			const agent = liveAgent(params.sessionId);
			const plan = ctx.get("planMode");
			if (plan === undefined) fail("control/unavailable", "this profile mounts no plan mode");
			return plan.get(agent);
		},

		async "plan.set"(params) {
			const agent = liveAgent(params.sessionId);
			const plan = ctx.get("planMode");
			if (plan === undefined) fail("control/unavailable", "this profile mounts no plan mode");
			return { result: plan.set(agent, params.active === true), state: plan.get(agent) };
		},

		async "goal.show"(params) {
			const agent = liveAgent(params.sessionId);
			const goals = ctx.get("goals");
			if (goals === undefined) fail("control/unavailable", "this profile mounts no goal service");
			const goal = goals.get(agent);
			return goal === undefined ? null : { ...goal };
		},
	};


	async function dispatch(request) {
		if (
			request === null ||
			typeof request !== "object" ||
			typeof request.method !== "string" ||
			(request.id !== undefined && !Number.isSafeInteger(request.id))
		) {
			return { id: request?.id ?? null, error: { code: "control/bad-request", message: "malformed request" } };
		}
		const handler = handlers[request.method];
		if (handler === undefined) {
			return { id: request.id ?? null, error: { code: "control/unknown-method", message: request.method } };
		}
		try {
			const result = await handler(request.params ?? {});
			return { id: request.id ?? null, result };
		} catch (error) {
			return { id: request.id ?? null, error: { code: codeOf(error), message: messageOf(error) } };
		}
	}

	/** Serialize one connection's requests so replies keep submission order. */
	function serve(socket) {
		let buffer = "";
		let chain = Promise.resolve();
		const reply = payload => {
			if (socket.destroyed) return;
			socket.write(`${JSON.stringify(payload)}\n`);
		};
		socket.on("data", chunk => {
			buffer += chunk.toString("utf8");
			for (;;) {
				const newline = buffer.indexOf("\n");
				if (newline < 0) break;
				const line = buffer.slice(0, newline);
				buffer = buffer.slice(newline + 1);
				if (line.trim().length === 0) continue;
				chain = chain.then(async () => {
					let request;
					try {
						request = JSON.parse(line);
					} catch {
						reply({ id: null, error: { code: "control/bad-json", message: "request is not JSON" } });
						return;
					}
					reply(await dispatch(request));
				});
			}
		});
		socket.on("error", error => {
			ctx.logger?.warn?.(`booth-control: client socket: ${messageOf(error)}`);
		});
	}

	const application = async () => {
		if (closed) return;
		rmSync(socketPath, { force: true });
		server = createServer({ allowHalfOpen: false }, serve);
		server.on("error", error => {
			ctx.logger?.warn?.(`booth-control: ${messageOf(error)}`);
		});
		const previousUmask = process.umask(0o077);
		try {
			await new Promise((resolve, reject) => {
				server.once("error", reject);
				server.listen(socketPath, () => {
					server.off("error", reject);
					resolve();
				});
			});
		} finally {
			process.umask(previousUmask);
		}
		try {
			chmodSync(socketPath, 0o600);
		} catch {
			// A restrictive umask may already enforce this.
		}
		ctx.logger?.info?.(`booth-control: listening on ${socketPath}`);
	};

	application().catch(error => {
		ctx.logger?.warn?.(`booth-control: cannot listen: ${messageOf(error)}`);
	});

	ctx.effect(
		() => async () => {
			closed = true;
			if (server !== undefined) {
				await new Promise(resolve => server.close(resolve));
			}
			rmSync(socketPath, { force: true });
			await Promise.allSettled([...active]);
		},
		"booth-control lifecycle",
	);
}
