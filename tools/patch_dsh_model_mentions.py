#!/usr/bin/env python3
"""Patch the installed dsh tool-subagent with kernel-side model-mention aliases.

dsh 0.2.1 exposes child provider/model selection only through deployment
settings.  Ring `^model` mentions need a per-conversation alias map instead:
the composer writes route-bearing `<model agent="m1" provider="…" model="…"/>`
tags into the user message, and this patch teaches the installed `subagent`
tool to list those aliases in the system prompt and resolve `agent: "m1"` to
the tagged route.  The source change lives in
`libs/dsh/packages/subagent/tool-subagent/src`; this script applies the same
change to an installed 0.2.1 package without rebuilding the monorepo.

Run it after installing/upgrading dsh. It is idempotent and keeps one backup.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys

PARSER = r'''
/** Read user-tagged model aliases from durable conversation history. */
function readModelMentions(session) {
	const found = new Map();
	for (const event of session.snapshotEvents()) {
		if (event.type !== "user/message") continue;
		for (const block of event.data.content) {
			if (block.type !== "text") continue;
			for (const match of block.text.matchAll(/<model\s+([^>]*?)\/>/gu)) {
				const attrs = {};
				for (const attr of match[1].matchAll(/([A-Za-z_][\w-]*)="([^"]*)"/gu)) attrs[attr[1]] = attr[2];
				const agent = attrs.agent;
				if (agent === void 0 || !/^m\d+$/u.test(agent) || !attrs.provider || !attrs.model || found.has(agent)) continue;
				found.set(agent, { agent, provider: attrs.provider, model: attrs.model, ...attrs.reasoning_effort ? { effort: attrs.reasoning_effort } : {}, ...attrs.name ? { name: attrs.name } : {} });
			}
		}
	}
	return [...found.values()];
}
function renderModelMentions(mentions, toolName) {
	if (mentions.length === 0) return "";
	return [
		"## User-tagged model mentions",
		`The user authorized these model aliases in this conversation. To delegate to one, call \`${toolName}\` with \`agent\` set to its alias (for example \`agent: "m1"\`); do not copy its provider or model fields by hand.`,
		...mentions.map(mention => `- ${mention.agent}${mention.name === void 0 ? "" : ` name="${mention.name}"`}: provider="${mention.provider}", model="${mention.model}"${mention.effort === void 0 ? "" : `, reasoning_effort="${mention.effort}"`}`)
	].join("\n");
}
'''


def package_root() -> str:
    dsh = shutil.which("dsh")
    if dsh is None:
        sys.exit("dsh not found on PATH")
    real = os.path.realpath(dsh)
    root = os.path.dirname(os.path.dirname(os.path.dirname(real)))
    return os.path.join(root, "dsh-tool-subagent", "lib", "index.js")


def patch(path: str) -> bool:
    with open(path, encoding="utf-8") as source:
        text = source.read()
    original = text
    if "function readModelMentions" not in text:
        text = text.replace('import { z as z$1 } from "zod";\n',
                            'import { z as z$1 } from "zod";\n' + PARSER, 1)
    if 'description: "Alias (m1, m2' not in text:
        text = re.sub(
            r'(\n(\s*)\.\.\.modelSelectionEnabled \? \{\n\s*provider: \{)',
            lambda m: (f'\n{m.group(2)}agent: {{\n{m.group(2)}\ttype: "string",\n'
                       f'{m.group(2)}\tdescription: "Alias (m1, m2, …) from a user ^model mention; resolves the route from this conversation."\n'
                       f'{m.group(2)}}},{m.group(1)}'),
            text, count=1)
    if "unknown model mention" not in text:
        text = re.sub(
            r'(\n(\s*)const agentPreset = presets\.resolve\(args\);\n\s*const modelRequest = args;)',
            lambda m: (
                f'\n{m.group(2)}const agentPreset = presets.resolve(args);\n'
                f'{m.group(2)}if (typeof args.agent === "string") {{\n'
                f'{m.group(2)}\tconst mention = readModelMentions(parent.session).find(candidate => candidate.agent === args.agent);\n'
                f'{m.group(2)}\tif (mention === void 0) throw new Error(`unknown model mention "${{args.agent}}"; mention a model in the prompt first`);\n'
                f'{m.group(2)}\targs = {{ ...args, provider: mention.provider, model: mention.model, ...mention.effort === void 0 ? {{}} : {{ reasoning_effort: mention.effort }}, ...args.name === void 0 ? {{ name: mention.agent }} : {{}} }};\n'
                f'{m.group(2)}}}\n'
                f'{m.group(2)}const modelRequest = args;'),
            text, count=1)
    text = text.replace('modelRequest, modelSelectionEnabled);',
                        'modelRequest, modelSelectionEnabled || typeof args.agent === "string");', 1)
    if 'tool:${toolName}:model-mentions' not in text:
        text = re.sub(
            r'(\n(\s*)runtimeCtx\.systemPrompt\.context\(\{\n\s*name: `tool:\$\{toolName\}:depth`)',
            lambda m: (
                f'\n{m.group(2)}runtimeCtx.systemPrompt.section({{\n'
                f'{m.group(2)}\tname: `tool:${{toolName}}:model-mentions`,\n'
                f'{m.group(2)}\torder: runtimeCtx.systemPrompt.getSectionOrder("TOOL_SUBAGENT"),\n'
                f'{m.group(2)}\ttext: (context) => context.agent === void 0 ? "" : renderModelMentions(readModelMentions(context.agent.session), toolName)\n'
                f'{m.group(2)}}});\n'
                f'{m.group(2)}runtimeCtx.on("session/event", (session, event) => {{\n'
                f'{m.group(2)}\tif (event.type === "user/message" && readModelMentions(session).length > 0) runtimeCtx.emit("system-prompt/change");\n'
                f'{m.group(2)}}});\n'
                f'{m.group(1)}'),
            text, count=1)
    if "unknown model mention" in text:
        print(f"already patched: {path}")
        return False
    if text == original:
        sys.exit(f"unsupported dsh-tool-subagent layout at {path}; build the patched libs/dsh source instead")
    backup = path + ".booth-model-mentions.bak"
    if not os.path.exists(backup):
        shutil.copy2(path, backup)
    with open(path, "w", encoding="utf-8") as target:
        target.write(text)
    check = subprocess.run(["node", "--check", path], capture_output=True, text=True)
    if check.returncode != 0:
        shutil.copy2(backup, path)
        sys.exit(f"patch produced invalid JavaScript; restored {backup}\n{check.stderr}")
    print(f"patched: {path}")
    return True


if __name__ == "__main__":
    patch(package_root())
