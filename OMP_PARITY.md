# OMP parity

`booth` is a dsh front end, not a second agent. Parity therefore covers the client-side surface
OMP owns: input, transcript rendering, commands, sessions, pickers, and terminal behavior. Agent
tools, providers, LSP/DAP, memory, and model routing stay with dsh.

## Implemented client-side parity

| Surface | booth |
| --- | --- |
| Full-screen TUI | Alternate screen by default, inline mode with `BOOTH_INLINE=1`, splash, composer chips, context gauge. |
| Editor | Undo/redo, word motions, kill ring with `Alt+Y`, multiline paste, external editor, `$python`/`!shell` local modes. |
| Vim | Insert/normal/visual/visual-line/replace, counts, motions, operators, text objects, dot repeat, undo, named/numbered/small-delete/black-hole registers. |
| Prompt | Submitted history, cleared-draft recall, Tab word completion, `@file` links, `^model` mentions, emoji, pending attachments. |
| Transcript | Markdown, tables, math, syntax fences, tool cards, diffs, todo HUD, errors, queue HUD, search, scrolling, mouse clicks/hover. |
| Commands | Fuzzy slash menu, aliases, argument completion, Markdown prompt templates from `.booth/commands` and the state directory. |
| Skills | `/skills` scans dsh project/user roots, lists user-invocable skills and inserts `/name`. |
| Providers | `/providers` (`/setup`) groups the live model catalog, marks the current provider and drills into a provider-filtered model picker. |
| Introspection | `/todo`, `/tools`, `/usage` and `/git` report local task, tool, context and working-tree state. |
| Sessions | List, resume, close, refresh, filter, relative age/preview, persistent pin/unpin. |
| Display | Theme overrides, live reload, notifications, mouse capture modes, effective-keybinding help. |
| Keys | One or more chords per action; `[]` disables an action; help renders the live bindings. |
| Clipboard | OSC 52 copy picker over prompts, answers, code, quotes, tools and shell commands, including tmux passthrough. |
| Links | Transcript and unsent-prompt URLs with hint-key picker; OSC-8 clicks. |

## Blocked by the dsh ACP surface

dsh `@deepseek-ai/dsh-acp` intentionally exposes only the standard automation protocol. These OMP
surfaces have no corresponding call, so booth does not fake them:

| OMP surface | Missing ACP capability |
| --- | --- |
| Agent Hub, live subagent transcripts | Only generic parent tool lifecycle is streamed. |
| Session tree, rewind, fork | `session/load` and fork are unsupported; resume returns a session without replay. |
| Session delete from the picker | Deletion is unsupported. |
| `/compact` | No compaction request. |
| `/collab`, `/share`, `/join` | No relay or guest transport. |
| MCP/plugin/marketplace management | No ACP management calls; profile configuration owns these. |
| `/move`, `/add-dir` | One workspace per session; additional directories are unsupported. |
| Provider OAuth sign-in, credential editing | No provider auth calls; dsh owns credentials. |
| Speech-to-text, live voice | No audio transport. |
| Cost/usage accounting | The wire carries context usage, not token cost. |

## Backend-owned, not duplicated

LSP, DAP, Python/Bun execution, web tools, memory, skills, subagents, and provider catalogs run in
dsh. booth renders their tool events and model/config options; it does not reimplement them.

## Verification

```sh
./tools/gate.sh
```

The gate builds, runs 441 unit/integration/PTY tests, and enforces the 600-line source limit.
