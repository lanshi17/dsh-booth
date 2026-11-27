# OMP parity

A per-command user-flow reference lives in [OMP_COMMANDS.md](OMP_COMMANDS.md).

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
| Control plane | A profile plugin opens `control.sock` beside ACP and executes dsh's native slash commands (`/compact`, `/goal`, `/plan`), session rename/delete/fork, tree/rewind, live agents, background jobs and the visible tool roster. |
| Skills | `/skills` scans dsh project/user roots; no argument opens the picker and inserts `/name`, while `search`/`installed` filter local roots and `install`/`update` report the missing skills.omp.sh registry. Skills also appear as searchable slash-menu entries and submit as prompt gestures. |
| Providers | `/providers` (`/setup`) opens the provider configuration list; Enter edits `baseURL` and API key, and `/providers edit <provider>` opens the editor directly. Secret values are never printed in the transcript. |
| Models | `/model` opens a two-pane provider/model browser with search and current-model marker; `/switch [model]` is normalized to the same picker and session-only path. |
| Settings | `/settings` toggles thinking, tool cards, tool output, notifications, mouse capture and Vim mode. |
| Introspection | `/todo` views, edits, imports, copies, exports and mutates the list (`append`, `start`, `done`, `drop`, `rm`, `expand`, `collapse`); `/tools`, `/usage [show|reset]`, `/agents` (including finished subagent sessions), `/jobs [full|kill <id>|kill all]`, `/tree` and `/git` report local task, tool, context, agent, job and working-tree state. |
| Git | `/git status|diff|stage|unstage|commit` runs git without a shell; `/commit [--all] [message]` commits staged changes, or stages everything first with `--all`. |
| MCP | `/mcp list|add|remove|test|enable|disable|reload` persists stdio/HTTP servers (disabled rows stay stored but are not passed to the next `session/new` or `session/resume`); OAuth and live resources are dsh-owned. |
| Context maintenance | `/compact [soft\|remote\|handoff]` maps OMP modes onto native compaction, `/shake` elides tool results, and `/clear`, `/new` and `/delete` reset local or durable state; `/clear` closes the live session so the next prompt starts a genuinely fresh dsh context instead of keeping stale history. |
| Automation | `/loop` repeats a prompt with count/duration and `--while`/`--until` shell gates; Esc pauses, session switches reset. |
| Model presets | `/modelpreset save|list|delete|switch` persists and applies model plus effort. `/switch` is the OMP session-only model command, not a preset alias. |
| Session relocation | `/move <path>` writes a seeded child log in the target directory and removes the old session; `/wt [branch]` creates `git worktree` beside the repository and relocates the session (starts from `HEAD`). |
| Plugins | `/plugins [list]` lists live plugin entries and bundles from dsh `pluginManager` (falling back to profile files); `enable`/`disable` call `setPluginEnabled`/`setBundleEnabled` and report a required restart; `/reload-plugins` reloads through the documented restart path. |
| Screen recording | `/record` toggles a JSONL frame recorder at `$BOOTH_STATE_DIR/recordings`; `booth play <file> [--speed N]` replays frames in the alternate screen. |
| Trace report | `/trace` writes a self-contained HTML report with entry timings, statuses and text, then opens it as a local file. The OMP stats server/sqlite dashboard is not bundled. |
| Changelog | `/changelog [full|last [N]]` parses the packaged `CHANGELOG.md` with the same recent/last/full selection rules. |
| Guided goal | `/guided-goal [rough objective]` queues the interview prompt into the parent agent and lets the native `create_goal` tool arm the goal. |
| Diagnostics | `/debug` reports the live projection counters; `/extensions` reports the dsh profile bundles and patch rows. |
| Side agents | `/btw <question>` awaits a spawned child's final answer without altering the parent transcript; `/tan <work>` leaves the child running in the background. |
| Handoff | `/handoff` runs dsh's native checkpoint compaction and labels the result a handoff checkpoint. The native prompt already records the primary request, current work and next step; a separate handoff prompt and focus instructions are not available. |
| Memory | `/memory` maintains a local Markdown backend at `$BOOTH_STATE_DIR/memory.md`, injects it as `<booth-memory>` prompt context, and supports view/add/stats/diagnose/queue/sync/clear/reset/enqueue/rebuild. There is no automatic extraction or mental-model bank. |
| Vibe | `/vibe <prompt>` runs a read-only side worker for one prompt; `/vibe on|off|status` tracks the preference. OMP's persistent worker session is not reproduced. |
| Fast / slow | `/fast` and `/slow` set `samplingParams.service_tier` on agent requests (`priority` / `flex`) through dsh's pi-ai adapter. This is provider-dependent; unsupported adapters ignore it. |
| Share / collab / join / leave | Minimal local HTTP share server: `/share [view|write]` returns a random-token URL, viewers poll transcript snapshots, `/collab` toggles queued prompts, `/join` returns the local URL, `/leave` stops. It is not protocol-compatible with the separately deployed dsh Liveshare server. |
| Extended context | `/extended-context on` resolves every catalog model's context window, selects the largest, and restores the previous model on `off`. It switches models rather than toggling a provider beta context feature. |
| Skillful | `/skillful on/off/status` hides or restores the `skill` tool for the session and injects a `<booth-skills-off>` instruction. Existing catalog history stays; this is not a full system-prompt catalog rewrite. |
| Force | `/force <tool>` restricts the next user turn to one visible global tool and restores the previous set at turn end. This enforces tool availability rather than sending the provider's `tool_choice`. |
| Plan review | `/plan-review` reviews the active plan's recent transcript with a read-only subagent and returns gaps, risks, missing verification and the top correction. It is not the OMP interactive review UI. |
| Account | `/login` starts DeepSeek browser OAuth through the profile webServer and opens the authorization URL; `/logout` removes the local grant. The callback is localhost-only. |
| Browser | `/browser status|install|on|off|headless|visible` installs and configures the experimental Playwright-MCP provider; activation requires `/reload-plugins`. |
| Computer | `/computer status|install|on|off` installs and configures the experimental CUA-driver provider; desktop permissions and platform support are provider-owned. |
| Advisor | `/advisor on/off/status/dump` runs a review subagent after each turn, stores terse notes, and injects them as `<booth-advisor>` context. It uses the same subagent model; no separate advisor model or configure editor. |
| Omfg | `/omfg <complaint>` forges a terse rule with a side agent and injects `$BOOTH_STATE_DIR/rules.md` as `<booth-rules>` prompt context. This is prompt-enforced, not an OMP TTSR hook. |
| Prewalk | `/prewalk [into]` arms a one-shot model switch; `restart [from] [into]` and `off` are supported. The handoff applies at the next user turn and restores the previous model at turn end. OMP's edit/write and todo-gated trigger is not reproduced. |
| Security | `/security scan [request] [--all]` starts a read-only audit subagent and stores a Markdown report under `$BOOTH_STATE_DIR/security`; `status/scans/show/cancel/plan` are available. Native SARIF import/export/validate/compare/disposition are not implemented. |
| Cleanse | `/cleanse [request] [--all]` spawns one diagnostics repair subagent that discovers, fixes and reruns project checks. OMP's weighted parallel file-disjoint worker fan-out is not implemented. |
| Marketplace | `/marketplace` manages JSON catalog sources, caches catalogs, discovers plugins, and installs/uninstalls/upgrades profile packages through npm; catalog `bundle` entries are added to `dsh.profile.bundles`. OMP marketplace manifests are not read. |
| Approvals | **Allow always** stores an exact tool/title/raw-input grant in `$BOOTH_STATE_DIR/approvals.json`; matching requests auto-answer with the offered allow-always option. `/forget-approvals` clears the store. |
| SSH hosts | `/ssh add|list|remove` maintains the OMP-compatible host JSON at `$BOOTH_STATE_DIR/ssh.json` and `$cwd/.booth/ssh.json`. dsh's SSH providers use OpenSSH aliases, so this is a client-side registry, not a connection layer. |
| Pause | `/pause` freezes the local dsh process with `SIGSTOP` and resumes with `SIGCONT`; remote kernel connections report unsupported. Separate ACP subagent processes are not covered. |
| Restart | `/restart` writes a session marker, closes ACP, re-execs booth with `/proc/self/cmdline`, and resumes the session on startup. |
| Stream refresh | `/fresh` closes and resumes the active session; the persisted transcript is replayed and process stream state is reset. |
| Workspace roots | `/add-dir`, `/remove-dir` and `/dirs` use dsh's session writable-root grants when the build exposes them (0.2.3+); older builds report `dirs/unsupported`. |
| Transcript export | `/export [--themes] [path]` writes self-contained HTML, or Markdown for `.md` paths; `/dump [all|anon]` JSON plus clipboard, `/open` last link, `/local` state listing. |
| Sessions | List, resume, close, refresh, filter, relative age/preview, persistent picker and `/pin`/`/unpin`; `/session info|delete` are available and `/session pin` reports that dsh owns credential accounts. |
| Session lifecycle | `/rename`, `/fork`, `/branch [seq]`, `/tree` and `/delete` use the harness's persisted session log and fork seed. |
| Display | Theme overrides, live reload, notifications, mouse capture modes, effective-keybinding help. |
| Keys | One or more chords per action; `[]` disables an action; help renders the live bindings. |
| Clipboard | OSC 52 copy picker over prompts, answers, code, quotes, tools and shell commands; direct `/copy answer|prompt|line|code|cmd|link|local`; image → image-path → text paste fallback; large pastes collapse to atomic markers. |
| Links | Transcript and unsent-prompt URLs with hint-key picker; OSC-8 clicks. |

## Command inventory

All 89 OMP builtin slash-command names resolve in booth, including aliases such as `/hub`,
`/subagents`, `/rewind`, `/worktree`, `/hotkeys`, `/resume`, `/setup`, `/exit` and `/q`.
`/live` and `/ratchet` are present and answer with an explicit capability notice instead of
falling through to the model as an unknown prompt. `commands_ui_test` enforces the full name list.

## Unavailable surfaces

dsh `@deepseek-ai/dsh-acp` intentionally exposes only the standard automation protocol; the
booth-control plugin adds the session and command operations above without changing that wire.
The surfaces below still have no harness capability behind them, so booth does not fake them:

| OMP surface | Missing ACP capability |
| --- | --- |
| Live subagent transcripts | Only generic parent tool lifecycle is streamed; `/agents`/`/hub` lists live and finished subagent sessions but not each child's transcript. |
| Speech-to-text, live voice | No audio transport. `/live` reports the missing transport instead of sending a prompt. |
| Cost/usage accounting | The wire carries context usage, not token cost; `/usage reset` reports that dsh exposes no provider reset credits. |
| `/ratchet` | No ratchet eval kernel. The command reports the missing kernel instead of sending a prompt. |

| `/trace` | The ACP surface carries events, not the stats server; `/trace` writes a static HTML report instead. |

## Backend-owned, not duplicated

LSP, DAP, Python/Bun execution, web tools, skills, subagents, and provider catalogs run in dsh. booth renders their tool events and model/config options; it does not reimplement them.

## Provider storage

OMP stores provider definitions in `~/.omp/agent/models.yml` (route, `baseUrl`, `api`,
`models`, compatibility) and credentials in the local SQLite auth store
(`~/.omp/agent/agent.db`, `auth_credentials`); `config.yml` holds settings, not
provider routes. booth does not write those files. Providers remain owned by dsh:
the `llm-pi-ai` settings namespace stores `providers.<route>.baseURL`, `apiKeyEnv`,
`api`, `models` and `compat`, while the secret is written through the dsh
credential store under that reference. `/providers` reads `providers.list` from
that namespace, so configuration works before any model or session exists, and
baseURL/API-key edits are visible to the next request without restarting booth.

## Verification

```sh
./tools/gate.sh
```

The gate builds, runs the unit/integration/PTY suite, and enforces the 600-line source limit.
