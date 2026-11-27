# OMP command UX reference

Every OMP 19.2.1 builtin slash command, the user flow it drives, and how `booth`
reproduces that flow on top of dsh. OMP source is the reference
(`packages/coding-agent/src/slash-commands/builtin-*.ts`); `booth` is the
implementation under `src/interaction/` and the `booth-control` profile plugin.

Notation: `✎` typed argument, `⏎` confirms, `↑/↓` moves, `Tab` completes,
`Esc` backs out. A flow is written as `command → state → output`.

## Reading this matrix

- **same** — the user-visible interaction and result match closely.
- **adapted** — the intent is preserved, but dsh exposes a different backend
  (local files, a control method, or a one-shot subagent) and the wording says so.
- **blocked** — dsh has no ACP/service capability for it; booth reports that
  explicitly instead of sending the text to the model or faking success.
- **gap** — client-side flow still missing; recorded here so it can be built.

No command falls through to the model as an unknown prompt. `commands_ui_test`
enforces that every OMP name and alias resolves.

## Session lifecycle and context

| Command | OMP user flow | booth flow | Parity |
| --- | --- | --- | --- |
| `/new` | ✎ command → new session → composer keeps focus | closes the active session and starts a fresh one; same when no session exists | same |
| `/fresh` | resets provider stream state, keeps the local transcript | closes and resumes the ACP session, replaying the persisted transcript | adapted |
| `/clear` | drops the conversation context in place, keeps the session id | clears the transcript and closes the ACP session; the next prompt opens a fresh context while the old session stays resumable. dsh cannot reset context under the same id | adapted |
| `/delete` | deletes the current session, starts a new one | `session.delete` (or close + fresh session) on the control plane | same |
| `/compact [mode] [focus]` | pick soft/remote/snapcompact/handoff, optional focus, report token delta | soft maps to dsh compaction, remote falls back locally, handoff is labelled, focus is reported as ignored, snapcompact reports the missing image archiver | adapted |
| `/handoff [focus]` | background handoff compaction with a summary | native `/compact`, labelled as a handoff checkpoint; focus text is ignored | adapted |
| `/shake [elide\|images\|thinking]` | prune tool results, images or thinking from context | `session.shake` with the mode; reports pruned results and bytes | same |
| `/resume [id\|@claude\|@codex]` | picker or id → switch session | `/sessions` picker (alias); foreign imports are not available | adapted |
| `/pin [id]` | pin a session at the top of the resume list | writes `session-pins`; `⏎` with no id pins the active session | same |
| `/unpin [id]` | remove a session pin | writes `session-pins` | same |
| `/rename [title]` | rename, or generate a title when empty | `session.rename`; empty title asks dsh for a generated title | same |
| `/move <path>` | move the session, seeded log, keep conversation | `session.move`; refuses while a turn is running | same |
| `/wt [branch]` | create a git worktree and move the session into it | `session.worktree`; refuses while a turn is running | same |
| `/restart` | re-exec with the same flags and resume | writes a restart marker, closes ACP, re-execs, resumes | same |
| `/add-dir <path>` | grant session writes under an extra directory | `sandboxPolicy.registerWritableRoot` on dsh 0.2.3+ | same |
| `/remove-dir <path>` | release an extra writable directory | `sandboxPolicy` release | same |
| `/dirs` | list workspace roots | cwd plus extra grants | same |
| `/exit`, `/quit`, `/q` | leave the TUI | exits the screen; the supervisor stays alive | same |
| `/session [info\|delete\|pin]` | info; delete current; pin an OAuth account | `info` and `delete` work; `pin` reports that dsh owns credentials | adapted |
| `/retry` | retry the last failed turn | resends the last prompt while idle | adapted |
| `/branch`, `/rewind` | pick a previous user message and branch there | lists turns, or `/branch <seq>` forks at that sequence | adapted |
| `/fork [pane\|window\|tab]` | fork the session; optionally open the fork in a tmux pane/window | forks the session; multiplexer placement is not available | adapted |
| `/tree` | navigate the session tree | `session.events` tree list | adapted |
| `/status` (booth) | OMP has no `/status`; the status line carries it | one-line phase/session/model/context summary | extra |

## Models, modes and automation

| Command | OMP user flow | booth flow | Parity |
| --- | --- | --- | --- |
| `/model`, `/models` | picker; `✎ id` sets the session model | two-pane provider/model picker; `✎ id` sets `session/set_config_option` | same |
| `/switch [model]` | session-only model change; no argument opens the picker | normalized to `/model`, so picker and `✎ id` behave identically | same |
| `/effort [level\|next]` | show levels or set thinking effort | shows current + levels; `next` cycles; sends `session/set_config_option` | same |
| `/modelpreset [list\|save\|switch\|delete] [name]` | persist role models + effort; switch applies them | JSON presets in the state dir; save/switch/delete/list | same |
| `/fast [on\|ultra\|off\|status]` | priority/ultrafast tier | sets `samplingParams.service_tier=priority`; `ultra` maps to priority | adapted |
| `/slow [on\|off\|status]` | flex tier (or Anthropic low priority) | sets `samplingParams.service_tier=flex` | adapted |
| `/plan [prompt]` | toggle plan mode; optional prompt starts planning | `plan.status` + `plan.set`, or native `/plan <args>` | adapted |
| `/plan-review` | reopen the latest plan review UI | read-only review subagent; returns gaps, risks, verification | adapted |
| `/goal [objective]`, `set/show/pause/resume/drop/budget` | toggle goal mode and manage the goal | maps `set/show/drop/pause/resume` onto native `/goal`; `budget` reports that dsh goals carry no token budget | adapted |
| `/guided-goal [rough]` | chat interview, then create a goal | queues the interview prompt; the native `create_goal` tool arms it | adapted |
| `/loop [count\|duration] [--while\|--until cmd] [prompt]` | repeat a prompt; Esc pauses | local loop controller with count/duration and shell gates; Esc pauses | same |
| `/queue <message>` | queue text for after the agent yields | while busy, queues; Alt+Up recalls the newest | same |
| `/vibe [prompt]` | toggle a persistent read-only worker | one-shot read-only side worker; `on/off/status` tracked | adapted |
| `/force <tool> [prompt]` | force the next turn's tool | hides every other tool for the next user turn, then restores | adapted |
| `/skillful [on\|off\|status]` | hide the skill listing for the session | hides the `skill` tool and injects `<booth-skills-off>` | adapted |
| `/extended-context [on\|off\|status]` | toggle larger context windows | selects the largest catalog model and restores the previous one on `off` | adapted |
| `/prewalk [into] \| restart [from] [into] \| off` | one-shot model handoff at the next edit/write | arms a one-shot switch at the next user turn; todo-gated trigger not reproduced | adapted |
| `/computer [on\|off\|status]` | toggle the computer-use prelude | persists `features.json`; install/on/off and CUA packages | adapted |
| `/browser [headless\|visible]` | toggle browser headless/visible | persists mode; `status/install/on/off/headless/visible` | adapted |
| `/ratchet [flow and goal]` | build an eval and hillclimb it | reports that dsh exposes no eval kernel; `.omp/ratchet` runs remain external | blocked |
| `/live` | Codex realtime voice | reports that dsh exposes no audio transport | blocked |
| `/pause` | freeze main, subagents and advisor | `SIGSTOP`/`SIGCONT` the local kernel; remote sessions report unsupported | adapted |

## Transcript, introspection and navigation

| Command | OMP user flow | booth flow | Parity |
| --- | --- | --- | --- |
| `/context` | browse context usage and content | one-line used / window / percent / bar / left | adapted |
| `/usage [show\|reset [target]]` | provider usage and limits; spend a reset credit | `show` = context + local counters; `reset` reports no provider reset credits | adapted |
| `/stats [--port\|--host]` | launch the sqlite stats dashboard | local transcript/tool counters; port/host are validated and the missing dashboard is reported | adapted |
| `/trace` | open the session in the stats dashboard | writes a self-contained HTML report and opens it | adapted |
| `/tools` | tools currently visible to the agent | `tools.list` roster | same |
| `/agents`, `/subagents` | Agent Hub with live and finished children | `agents.list`, including finished sessions from the persisted log | adapted |
| `/hub` | alias for the live Agent Hub | alias for `/agents` | adapted |
| `/jobs [full\|kill <id>\|kill all]` | async job list; full captions; kill one/all | `jobs.list` + `jobs.kill`; `full` shows untruncated labels and detail; `kill all` supported | same |
| `/changelog [full\|last [N]]` | recent, last N, or full changelog | same selection rules over packaged `CHANGELOG.md` | same |
| `/hotkeys` | all keyboard shortcuts | `/help` renders live bindings; aliases `keys` | same |
| `/extensions` | Extension Control Center (OMP aliases it as `status`) | profile bundles, dependencies and patch rows. booth keeps `/status` for its phase/session summary instead of shadowing it | adapted |
| `/settings` | settings menu | filterable toggles: thinking, tool cards, output, notifications, mouse, Vim | adapted |
| `/setup`, `/providers` | provider setup | provider list; `⏎` edits baseURL/API key; `/providers edit <p>` direct | same |
| `/debug` | debug tools selector | phase/session/model/effort/entry/tool/thought/todo counters | adapted |
| `/local [name]` | edit a `local://` file; picker when empty | lists/opens files in the booth state directory | adapted |
| `/git [revision]` | split diff viewer, staging, commit composer | `/git` opens the working tree, a revision shows its patch, and status/diff/stage/unstage/commit run without a shell | adapted |
| `/commit [instructions]` | commit only this agent's changes | `/commit [--all] [message]` stages and commits | adapted |
| `/copy` | picker; `code`, `cmd`, `link`, `local` shortcuts | picker plus `answer`, `prompt`, `line`, `code`, `cmd`, `link`, `local` | same |
| `/open [link]` | open the last link | opens the last transcript link; validates `link`/`url` | same |
| `/export [--themes] [path]` | HTML export | self-contained HTML; `.md` path writes Markdown | same |
| `/dump [all\|anon]` | clipboard text + LLM request JSON; `all`/`anon` archives | JSON file + clipboard Markdown; `anon` redacts text | adapted |
| `/record` | start/stop replayable screen recording | JSONL frames under `$BOOTH_STATE_DIR/recordings`; `booth play` replays | adapted |

## Collaboration and sharing

| Command | OMP user flow | booth flow | Parity |
| --- | --- | --- | --- |
| `/share` | encrypted share link or gist | local HTTP share server with a random token; not protocol-compatible | adapted |
| `/collab [start\|view\|list\|stop\|status]` | relay host; view-only/control links | local share server; same verbs; `list` shows process-local hosts | adapted |
| `/join <link>` | join a relay session as guest | opens/returns the local share URL; remote relay join unavailable | adapted |
| `/leave` | leave a collab session | stops the local share | adapted |

## Skills, MCP and providers

| Command | OMP user flow | booth flow | Parity |
| --- | --- | --- | --- |
| `/skills [search\|install\|installed\|update]` | registry search/install/update | no argument opens the local picker; `search` and `installed` cover local roots; `install/update` report the missing registry | adapted |
| `/ssh [add\|list\|remove\|help]` | OMP-compatible host JSON | same host JSON in state and project roots; connections stay with dsh | adapted |
| `/mcp [add\|list\|remove\|test\|enable\|disable\|reload]` | manage stdio/HTTP servers and OAuth | config-only: `add <name> [--url <url>\|-- <cmd...>]`, list/remove/test/enable/disable/reload; auth and live resources are dsh-owned | adapted |
| `/plugins [list\|enable\|disable]` | view and enable/disable plugins | lists live plugin entries and bundles from dsh's `pluginManager` (falling back to profile files); `enable`/`disable` apply to a plugin entry or bundle and report restart requirements | same |
| `/reload-plugins` | reload plugins in place | restarts through the documented startup-only composition path | adapted |
| `/marketplace add/remove/update/list/discover/install/uninstall/installed/upgrade/help` | catalog sources and npm packages | JSON catalog cache + npm install/uninstall; OMP manifests are not read | adapted |
| `/login [provider\|URL]` | OAuth provider flow | DeepSeek browser OAuth through the profile webServer; other providers and manual redirect URLs report unavailable | adapted |
| `/logout [provider]` | sign out of an OAuth provider | clears the DeepSeek grant; provider arguments are forwarded and unsupported providers report unavailable | adapted |
| `/advisor [on\|off\|status\|dump\|configure]` | second model reviews each turn | review subagent after each turn; notes injected as `<booth-advisor>` | adapted |
| `/memory [view\|add\|stats\|diagnose\|queue\|sync\|clear\|reset\|enqueue\|rebuild\|mm ...]` | memory bank maintenance | local Markdown backend; `mm` mental models report unavailable | adapted |
| `/omfg <complaint>` | forge a TTSR rule | side agent forges a terse rule; injected as `<booth-rules>` | adapted |

## Workflow helpers

| Command | OMP user flow | booth flow | Parity |
| --- | --- | --- | --- |
| `/todo [edit\|copy\|expand\|collapse\|export\|import\|append\|start\|done\|drop\|rm]` | HUD + Markdown round-trip + fuzzy mutations | HUD with 3-row preview / full expansion; edit/import through `$EDITOR`; append/start/done/drop/rm; drop removes because dsh has no abandoned status | adapted |
| `/btw [question]` | ask a side question, or browse this session’s BTW history | `side.ask`; answer awaited and shown, parent transcript unchanged; no argument lists this session's `btw-history.jsonl` entries | same |
| `/tan <work>` | background tangent agent | `side.background`; returns the child id | same |
| `/cleanse [request] [--all]` | weighted parallel repair workers | one diagnostics repair subagent; reports diagnostics/files/verification | adapted |
| `/security <plan\|scan\|status\|cancel\|scans\|show\|...>` | native scan store, SARIF import/export/compare | scan/status/scans/show/cancel/plan; SARIF operations report unavailable | adapted |
| `/forget-approvals` | drop remembered approvals | deletes `approvals.json`; the next read/write asks again | same |

## Exit and editor surfaces

| Command | OMP user flow | booth flow | Parity |
| --- | --- | --- | --- |
| `/quit`, `/q`, `/exit` | quit | quits the TUI and saves the draft | same |

## Implementation notes

1. **One dispatcher order.** TUI: control commands → file command templates →
   provider edit → library overlay (`/skills`, `/providers`, `/settings`) →
   local commands → picker/attachment specials → unknown. A line is consumed
   by exactly one layer.
2. **`/switch` is normalized to `/model`.** `normalize_command` rewrites the
   token before dispatch, so the picker and argument completion are shared.
   `/modelpreset switch` remains the persisted-preset path.
3. **Control-backed mutations commit immediately.** `/todo` mutations update
   the local HUD and call `todos.commit`, so the agent and the HUD cannot drift.
4. **Blocked surfaces state the missing capability.** `/live`, `/ratchet`,
   MCP OAuth/live resources, provider reset credits and foreign session imports
   answer with a capability notice. They never become model prompts.
5. **`/clear` prefers context correctness.** dsh has no in-place context reset,
   so `/clear` closes the live session and lets the next prompt open a new one;
   the old session remains in the resume list.
6. **The matrix is executable.** `tests/commands_ui_test.cpp` checks every OMP
   name and alias, `tests/extra_commands_test.cpp` covers MCP/config flows, and
   the PTY suite (`tests/pty_*.sh`) drives the real TUI against the fake kernel.
