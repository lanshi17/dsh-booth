# booth

A terminal console for one dsh profile. `booth` starts `dsh --profile booth` and drives it over
ACP, showing the conversation and streaming the agent's work. There is no second agent layer and
no router: dsh is the agent, booth is its front end. [OMP_PARITY.md](OMP_PARITY.md) records the
client-side parity matrix and the surfaces blocked by dsh's ACP contract.

## Build

Requirements: CMake 3.28+, Ninja, a C++23 compiler (GCC 15+), ICU (`icu-uc`), and network
access for the pinned FetchContent dependencies (CLI11, nlohmann/json, Catch2, FTXUI).

```sh
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Debug
cmake --build build
ctest --test-dir build
tools/gate.sh            # build + ctest + size gate
```

## Install

```sh
cmake --build build
./install.sh
```

`install.sh` installs the binary. On first start `booth` creates the `booth` dsh profile (the
`dsh-base` and `dsh-acp-app` bundles) under `$DSH_HOME/profiles/booth` if it is missing.

## Use

```sh
booth up                                 # detached kernel + ACP socket
booth run "fix the failing test" --cwd . # one task through the socket (or in-process)
booth sessions [--json]
booth status [--json]
booth logs --lines 40
booth down                               # stop the daemon
booth                                    # the TUI, when stdin is a terminal
```

The TUI follows omp's shape locally: full-screen alternate screen with the transcript above a
two-row composer. Over SSH it defaults to inline terminal output and leaves mouse capture off, so
the local terminal owns scrollback, native selection and copy; `BOOTH_INLINE=1`/`0` forces either
mode and `/mouse on` re-captures the mouse. The composer's top border is a dark chip band with
the model, reasoning effort when dsh reports it, cwd, git branch/dirty counts, working timer and
a threshold-colored context gauge; the bottom row merges
the prompt with the key hints. User turns keep omp's full-width band, assistant turns render
markdown (lists, links, tables, inline code, LaTeX math, syntax-highlighted fences), and tool calls render as
full-width frames with input, output, diff statistics and duration; paragraphs wrap at the terminal
edge, breaking CJK text between glyphs and long tokens (paths, URLs, code lines) instead of clipping
them, and closing CJK punctuation stays attached to the glyph before it. Shell cards show the last 10
output lines and the exit status, grep cards group matches by file, glob cards group paths by
directory, web search cards show the answer and clickable sources, web fetch cards show the URL,
HTTP status, preview and truncation notice, skill cards show the base directory and instructions,
spill notices stay visible with omitted bytes and the recovery path, read cards use a dim line-number
gutter, generic JSON tool output pretty-prints as an indented tree, and a diff shows old/new line
numbers, stopping at 40 lines until `Ctrl+O`; edit/write cards title the path with +/− stats, and
`apply_patch` cards split per file with A/M/D labels and per-file +/− stats. `/copy` opens a
filterable picker over prompts, answers, fenced code, blockquotes, tool
output and shell commands, copying with OSC 52 (tmux passthrough included) and, on a local
session, through `wl-copy`/`xclip`/`pbcopy` as a fallback. Over SSH native clipboard reads are
skipped because they belong to the remote host; use the terminal's paste chord. If the terminal
blocks OSC 52, run inline (`BOOTH_INLINE=1`) with `/mouse off` and select/copy through the
terminal's native scrollback. `/export [--themes] [path]` writes a self-contained HTML
transcript (default: `$BOOTH_STATE_DIR/transcript-<timestamp>.html`); a `.md` or `.markdown`
path writes Markdown instead. `Enter` sends, `Alt+Enter` inserts a newline, `Tab` completes the word at the caret from prompt history and committed user/assistant transcript text (unless a command, file, or model-mention menu is open), `ArrowUp`/`ArrowDown` walk cleared drafts and prompt history,
`Ctrl+C` cancels the run or clears an idle draft (Up recalls it); a second consecutive Ctrl+C exits, `Ctrl+D` quits and saves the unsent draft for the next start, `Alt+R`
retries the last prompt while idle, `Ctrl+R` searches prompt history, `Ctrl+G`
opens `$VISUAL`/`$EDITOR` on the prompt (`booth.editor` in `keybindings.yml` moves that key),
`PageUp`/`PageDown` or the wheel browse the transcript, `Shift+Tab` cycles dsh's reasoning
effort. The editor supports `Ctrl+Z` undo, `Alt+Z` redo, `Ctrl`/`Alt`+`←`/`→` word movement,
`!command` runs a local shell command during any turn and `$python` runs Python locally (an
expression prints its value); neither is sent to the model or saved as prompt history.
`Ctrl+W`/`Ctrl+U`/`Ctrl+K` kill and `Ctrl+Y` yank; `Alt+Y` replaces the yank with an older kill.
`Alt+Up` takes a queued prompt back into
the editor. Drafts taller than eight rows scroll internally with a `↑N` marker; a permission card
stays above the composer with the transcript visible, reuses the matching tool card's title and
input, and shows only the choices dsh advertised
(`y` once, `a` always, `n` deny, `d` deny always when offered). `Enter` arms an allow and requires a
second press after half a second; `Ctrl+O` expands the full diff.
Commands: `/status`, `/model`, `/switch`, `/providers` (`/setup`), `/skills`, `/settings`, `/sessions`, `/new`, `/close`,
`/copy`, `/clear`, `/retry`, `/export`, `/effort`, `/theme`, `/notify`, `/links`, `/mouse`,
`/context`, `/usage`, `/stats`, `/todo`, `/tools`, `/agents`, `/jobs`, `/git`, `/pin`, `/unpin`,
`/queue`, `/session`, `/acp`, `/cancel`, `/find`, `/attach`, `/detach`, `/help`, `/quit`,
`/compact`, `/handoff`, `/goal`, `/plan`, `/rename`, `/fork`, `/branch`, `/tree`, `/delete`, `/commit`,
`/open`, `/dump`, `/local`, `/mcp`, `/modelpreset`, `/switch`, `/loop`, `/add-dir`,
`/remove-dir`, `/dirs`, `/fresh`, `/move`, `/wt`, `/restart`, `/reload-plugins`, `/btw`, `/tan`,
`/guided-goal`, `/debug`, `/extensions`, `/plugins`, `/record`, `/changelog`, `/trace`, `/pause`, `/ssh`,
`/forget-approvals`, `/memory`, `/marketplace`, `/cleanse`, `/security`, `/prewalk`, `/omfg`, `/advisor`,
`/login`, `/logout`, `/browser`, `/computer`, `/plan-review`, `/force`, `/skillful`, `/extended-context`,
`/share`, `/collab`, `/join`, `/leave`, `/fast`, `/slow`, `/vibe`, `/live`, `/ratchet`, `/paste`. `/plugins` lists live plugin entries and bundles from dsh's plugin manager (falling back to profile files); `enable|disable <name>` applies through the manager and reports a restart when the profile has no HMR.
Every builtin command name and its OMP user flow are catalogued in
[OMP_COMMANDS.md](OMP_COMMANDS.md). `/notify` reports the setting; `/notify on` and
`/notify off` persist terminal notifications in `$BOOTH_STATE_DIR/notify`. Kernel and protocol
failures pin a red `error · /retry` banner above the composer until the next prompt. Notifications default to
on and use OSC 9 for permission requests and turns lasting at least five seconds (tmux passthrough
is supported).
Aliases: `/resume`, `/models`, `/reasoning`, `/hotkeys`, `/keys`, `/exit`, `/q`.
`/copy`, `/clear`, `/export`, `/status`, `/context`, `/stats`, `/effort`, `/theme`, `/queue`,
`/session`, `/acp`, `/cancel`, `/notify`, `/links`, `/mouse`, `/find`, `/attach`, `/detach`,
`/skills`, `/providers`, `/settings`, `/todo`, `/tools`, `/agents`, `/jobs`, `/git`, `/pin`,
`/unpin`, `/compact`, `/handoff`, `/goal`, `/plan`, `/rename`, `/fork`, `/branch`, `/tree`, `/delete`, `/commit`,
`/open`, `/dump`, `/local`, `/mcp`, `/modelpreset`, `/switch`, `/loop`, `/add-dir`,
`/remove-dir`, `/dirs`, `/fresh`, `/move`, `/wt`, `/restart`, `/reload-plugins`, `/btw`, `/tan`,
`/guided-goal`, `/debug`, `/extensions`, `/plugins`, `/record`, `/changelog`, `/trace`, `/pause`, `/ssh`,
`/forget-approvals`, `/memory`, `/marketplace`, `/cleanse`, `/security`, `/prewalk`, `/omfg`, `/advisor`,
`/login`, `/logout`, `/browser`, `/computer`, `/plan-review`, `/force`, `/skillful`, `/extended-context`,
`/share`, `/collab`, `/join`, `/leave`, `/fast`, `/slow`, `/vibe`, `/help` and `/quit` stay available
while a turn is running; a plain prompt typed during a run is queued, shown above the composer,
and sent when the run ends. An open `todo_write` list stays above the composer as a bounded
`Todo done/total` HUD and disappears once every item is complete.
`/providers` opens the provider list from dsh's `llm-pi-ai` directory, so it works before any
session or model exists. `Enter` opens an in-TUI provider page with `name`, `displayName`, `api`,
`baseURL`, comma-separated `models` and `apiKey`; `Tab`/arrows move fields, `Ctrl+S` saves,
`Ctrl+F` discovers endpoint models, `Ctrl+X` removes the user layer and its key, and `Esc` cancels.
The last row, `＋ new provider`, creates a route. Blank `apiKey` keeps the current key,
`apiKey=!clear` removes it, and blank `api`/`models`/`baseURL` restore catalog defaults. Saving
writes `llm-pi-ai` settings and stores keys through dsh credentials. OMP instead writes routes to
`~/.omp/agent/models.yml` and credentials to its SQLite auth store; booth keeps dsh as the owner.
`/model` is separate and
opens the two-pane provider/model browser.
`/settings` opens a full-screen toggle list for thinking, tool cards, tool output, notifications,
mouse capture and Vim mode; notification and mouse changes persist.
`/skills` registers user-invocable skills in the slash menu, so typing `/rev` finds `/review`;
it scans the dsh skill roots (`<project>/.dsh/skills`, `<project>/.agents/skills`,
`$DSH_HOME/skills`, `$DSH_AGENTS_HOME/skills`), lists user-invocable skills, and inserts `/name `
into the composer. A `/name` skill token is submitted to dsh as a prompt gesture rather than
treated as an unknown local command. Provider credentials stay with dsh; booth does not implement
OAuth.
`/model` opens a two-pane browser: providers on the left, models on the right; type to search (↑/↓ or wheel, Enter); `Ctrl+P` cycles to the
next advertised model, and the selection is applied through the ACP config option. `/effort` changes dsh's reasoning effort: `next` cycles, a level
name or wire value selects it, and no argument lists the current value and choices. `/theme`
reloads `$BOOTH_STATE_DIR/theme.conf` into the running TUI; `/theme key=#rrggbb` validates,
persists, and applies one color override immediately. Supported keys are `accent`, `text`, `dim`,
`line`, `panel`, `border`, `green`, `yellow`, `red`, `git`, `gray`, `token`, and `hover`. Invalid
keys or colors leave the file unchanged and show a notice.
`/context` reports dsh's context window usage; `/usage` adds transcript counters, `/stats` counts
local transcript entries, tools and queued prompts, and `/todo` reports the todo HUD (`/todo copy`
or `/todo export [path]` writes Markdown; `/todo edit` opens `$VISUAL`/`$EDITOR`, and
`/todo import [path]` replaces the session list from Markdown). `/tools` lists the tools the live
agent can see, `/agents` (`/hub`, `/subagents`) lists live sessions plus finished subagent
sessions, and `/jobs` lists or kills background jobs.
`/git` runs `status`, `diff [staged]`, `stage <path>`, `unstage <path>` or `commit [message]` in
the working directory without a shell. `/commit [message]` commits the staged changes; add
`--all` to stage every change first.
`/new` closes the current session and leaves a clean transcript, while `/clear` only clears the
local transcript.

The control plane (`plugins/booth-control.mjs`, installed by the supervisor into the profile)
executes dsh's own slash commands and session operations. `/compact` runs the native compaction
command, `/goal` maps `show|set|pause|resume|drop` onto it, and `/plan` toggles plan mode.
`/rename [title]` accepts a title or materializes the fallback title; `/fork` creates a seeded
child from the latest completed turn and resumes it, `/branch [seq]` forks at an event sequence
(`/branch list` shows them), `/tree` lists conversational events, and `/delete` closes and removes
the current session, then opens a fresh one. `/mcp list|add <name> <command...|url>|remove|test|enable|disable|reload`
persists MCP servers in `$BOOTH_STATE_DIR/mcp.json`; disabled rows stay stored but are not passed
to the next `session/new` or `session/resume`. `/modelpreset save|list|delete|switch` stores the
current model and effort in `$BOOTH_STATE_DIR/model-presets.json`; `/switch [model]` is the OMP
session-only model command and shares `/model`'s picker, while `/modelpreset switch <name>`
applies a saved preset. `/loop [count|duration]`
`[--while|--until '<cmd>'] [prompt]` repeats the next prompt, with the same count/duration and
shell-condition grammar as OMP; Esc or `/cancel` pauses it, `/loop` disables it, and a session
switch resets it. `/add-dir <absolute path>` grants session writes under an extra directory
through dsh's sandbox policy, `/remove-dir` releases it, and `/dirs` lists the workspace root and
extra grants; this needs a dsh build with `sandboxPolicy.registerWritableRoot` (0.2.3 or newer). `/fresh` closes and resumes the active session, resetting the agent's
provider stream state while replaying the persisted transcript. `/move <path>` relocates the
session into another directory by writing a seeded child log and deleting the old one; `/wt
[branch]` (alias `/worktree`) creates a git worktree beside the repository and moves the session
there. Both refuse to run during a live turn, and `/wt` starts from `HEAD` (uncommitted source
changes are not carried over). `/restart` writes a restart marker, closes the active session and
re-execs booth with its original argv; the new process resumes the marked session. `/btw <question>` runs a separate spawned child agent and prints its final answer without touching the
parent transcript; `/tan <work>` starts the same kind of child in the background and reports its
session id. Both use dsh's `spawn` subagent provider and inherit the parent's model route. `/debug` prints
the live phase/session/model/effort/entry counters; `/extensions` prints the dsh profile's
bundles, dependency count and patch-row count. `/guided-goal [rough objective]` injects the
interview kickoff into the live agent; the agent asks one question per turn and calls
`create_goal` with the five-field objective structure when ready. `/record` toggles a JSONL screen
recording under `$BOOTH_STATE_DIR/recordings`; each frame carries its size, timestamp and rendered
text. `booth play <file> [--speed N]` replays the frames in the alternate screen (`--speed 0` prints
without delays). `/changelog [full|last [N]]` reads the packaged `CHANGELOG.md`; no argument shows
the three newest releases, `last N` shows N, and `full` shows everything. `/plugins [list]` lists the
profile's dependency versions and bundle names; enable/disable requires a profile edit plus
`/reload-plugins`. `/handoff` runs native checkpoint compaction and reports a handoff checkpoint;
dsh has no separate handoff prompt, so focus instructions are ignored. `/trace` writes a self-contained HTML report with per-entry timing, status and
text under `$BOOTH_STATE_DIR/traces` and opens it as a local file. `/pause` sends `SIGSTOP` to the
local dsh kernel and a second `/pause` sends `SIGCONT`; remote daemon kernels report unsupported.
`/ssh add|list|remove` maintains OMP-compatible host JSON at `$BOOTH_STATE_DIR/ssh.json` (user) and
`$cwd/.booth/ssh.json` (project). dsh's mounted SSH providers use OpenSSH aliases; this registry is
the client-side config surface, not a connection layer. Choosing **Allow always** remembers an exact
tool/request grant in `$BOOTH_STATE_DIR/approvals.json`; matching requests are auto-approved.
`/forget-approvals` removes the store, so the next request asks again. `/memory` keeps a local
`memory.md`: `view/add/stats/diagnose/queue/sync/clear/reset`; the file is injected as a
`<booth-memory>` system-prompt context. `/marketplace` manages JSON catalog sources under
`$BOOTH_STATE_DIR/marketplaces.json`, caches under `marketplace/`, and installs catalog packages
with `npm` into the booth profile; installed entries with a `bundle` are added to the profile
bundles. `/cleanse [request] [--all]` spawns a diagnostics repair subagent that runs the
repository's own build/test/typecheck/lint commands, fixes failures, reruns them, and reports
Diagnostics/Files changed/Verification/Remaining. `/security scan [request] [--all]` starts a
read-only audit subagent, stores its report under `$BOOTH_STATE_DIR/security`, and supports
`status/scans/show/cancel/plan`. `/prewalk [into]` arms a one-shot model switch that applies on
the next user turn and restores the previous model at turn end; `restart [from] [into]` selects
`from` and arms `into`, and `off` cancels a pending switch. `/omfg <complaint>` asks a side agent
to forge a terse behavioral rule, appends it to `$BOOTH_STATE_DIR/rules.md`, and injects it as a
`<booth-rules>` prompt context. `/advisor on|off|status|dump` enables a review advisor that runs
after each turn, stores terse notes, and injects them as `<booth-advisor>` context. `/login` starts DeepSeek account OAuth through the local webServer and
opens the authorization URL; `/logout` clears the local grant. `/browser` and `/computer` manage the
dsh provider packages: `status`, `install`, `on`, `off`, plus `headless|visible` for the browser.
Provider installation writes `$BOOTH_STATE_DIR/features.json`; the supervisor adds the matching
profile rows, and `/reload-plugins` restarts to activate them. `/plan-review` asks a review
subagent to critique the active plan's recent transcript and returns actionable bullets; it
requires plan mode. `/force <tool>` restricts the next user turn's visible global tools to that
single tool, then restores the previous tool set at turn end; `/force off` cancels a pending force. `/skillful off` hides the `skill` tool for the session and
injects a `<booth-skills-off>` instruction; `/skillful on` restores both. `/extended-context on` scans
the model catalog for the largest known context window, switches to it, and `/extended-context off`
restores the previous model. `/share [view|write]` starts a minimal local HTTP share server on
`127.0.0.1`; `/collab on|off` toggles whether viewers may queue messages, `/join [url]` shows the
local share URL, and `/leave` stops it. The URL contains a random token; viewers poll a read-only
transcript and, in write mode, can post prompts. This is a self-contained local relay, not the
separate dsh Liveshare server protocol. `/fast on|ultra|off|status` and `/slow on|off|status` set the
provider `service_tier` body field to `priority` or `flex` through `samplingParams`; the installed
pi-ai adapter is patched by `tools/patch_dsh_service_tier.py` during install. Providers that do not
accept the field ignore it. `/vibe <prompt>` starts a read-only side worker for one prompt;
`/vibe on|off|status` tracks the mode preference. `/dump [all|anon]` writes a JSON transcript to `$TMPDIR` and copies the
Markdown; `/open` opens the last transcript URL; `/local [file]` lists the state directory or opens one file with the platform opener.
`/queue <text>` enqueues a prompt for when the active turn ends; `Alt+Up` brings the newest
queued prompt back into the editor. `/session` (or `/session info`) prints the live ACP session (id, title, model,
effort, context, queue), and `/acp` prints the protocol, phase, connection and socket.
`/attach <path>` adds an ACP `resource_link` to the next prompt (shown as a `file N` status chip);
`/detach [name|all]` clears pending links, and `/cancel` is the explicit ACP cancel command.
`/find <text>` searches the visible transcript and cycles older matches on repeated calls; cards
hidden by thinking or tool toggles are skipped, so a match always lands on a drawn row.
`/links` (or `Alt+L`) lists openable URLs from user, assistant and tool output, plus the unsent prompt, with `a`–`z`/`A`–`Z`
hint keys; Enter or a
hint opens the selection and Esc closes. `/mouse` reports capture; `/mouse on|off` persists it.
Capture defaults off inside tmux so wheel scrollback and native/tmux selection copy work; turn it
on for booth's wheel scrolling, clicks and hover. `/copy` still writes through tmux's OSC 52
passthrough. `/help` (or `Alt+H`) lists every command and alias and renders the effective
`keybindings.yml` chords; the card scrolls with ↑/↓, PageUp/PageDown, Home/End and the wheel, and
Esc or `q` closes it.
A prompt starting with `!` runs the rest as a local `/bin/sh -c` command in the working directory,
shows a `bash` tool card, and keeps the command/output out of model context and prompt history.
`Ctrl+R` searches prompt history; `Ctrl+O` expands tool output, `Ctrl+T` shows or hides thinking
(hidden by default), and `Alt+O` (or `Ctrl+Shift+O` on terminals that report it) hides or shows tool
cards. When shown, a finished thought collapses to `Thought for Ns` and expands when clicked;
clicking a tool row toggles its expansion. Mouse clicks also select and activate slash, file, model,
session, history, copy and model-mention rows, place the composer caret, open OSC-8 links, answer
permission buttons and dismiss help; moving the pointer paints a hover band. Bracketed multi-line pastes are inserted as one edit and newlines never submit. Pastes longer than
1000 bytes or 10 lines collapse to `[Paste #N, +L lines]` / `[Paste #N, C chars]`; Backspace or
Delete removes the whole marker, and sending expands it back to the original text.
`Ctrl+V` prefers a valid clipboard raster (magic bytes checked, so text returned for an image
target is not attached), then a clipboard path to a PNG/JPEG/WebP/GIF file, then clipboard text
(shown as an `image N` chip for images);
`:smile:`-style shortcodes expand when the prompt is sent. Set
`BOOTH_VIM=1` for OMP-parity modal editing: insert/normal/visual/visual-line/replace, counts,
`h j k l 0 ^ $ w b e f F t T ; , gg G % { }`, `d/y/c/>/<`, `dd/yy/cc`, `D/C/x`, `p/P`, `u`,
`.`, `J`, `r/R`, text objects (`iw aw iW aW`, quotes, brackets, `ip/ap`), desired column and
sticky `$`. Registers `"a`–`"z` (append with `"A`–`"Z`), `"0`, `"1`–`"9`, `"-` and `"_` work with
yank, delete and put; Vim yanks and deletes also feed the `Ctrl+Y` kill ring. Type `^` anywhere in the prompt for
model autocomplete; known mentions become stable `mN` tags carrying their route. The patched dsh
`subagent` tool lists those aliases in the system prompt and accepts `agent: "m1"`, resolving the
route in the kernel; aliases persist in the conversation tags and rebuild on resume. `/sessions`
lists saved sessions with the first prompt and relative age: Enter resumes, `r` refreshes, `c`
closes the active session, and `Alt+K` pins or unpins the selection; pinned sessions stay first.
`/pin [id]` and `/unpin [id]` toggle the active session (or an explicit id) from the composer. Markdown files in `.booth/commands/` (project) and `$BOOTH_STATE_DIR/commands/` (user) become
slash commands. Frontmatter `description` labels the menu row; `$ARGUMENTS`/`$@` expand to the
whole argument string and `$1`–`$9` to words, and unused arguments are appended. Project commands
win on name collisions. Type `/` for the fuzzy command menu below the input: names, aliases and descriptions all
match. ↑/↓ or the wheel moves, PgUp/PgDn/Home/End jump, Tab completes the selected command, Enter runs the selection
and Esc dismisses the menu without clearing the draft. For `/notify`, `/mouse`, `/effort` and
`/model`, typing a space keeps the menu open over argument values; Tab completes one and Enter submits it. The wheel also moves the model and session
pickers.

## Environment

| Variable | Default |
| --- | --- |
| `BOOTH_DSH_BIN` | `dsh` on `PATH` |
| `BOOTH_PROFILE` | `booth` (state dir name) |
| `BOOTH_BASE_PROFILE` | `booth` (the dsh profile booth starts) |
| `BOOTH_STATE_DIR` | `$DSH_HOME/state/booth` |
| `DSH_HOME` | `$HOME/.dsh` |
| `BOOTH_HANDSHAKE_TIMEOUT_S` | `20` |
| `BOOTH_VIM` | unset (Vim mode off) |
| `BOOTH_INLINE` | unset (full-screen locally, inline over SSH; `1`/`0` forces it) |
| `BOOTH_CLIPBOARD_TMUX` | unset (force tmux passthrough when the multiplexer hides `TMUX`) |
| `OPENCODE_API_KEY` | unset (OpenCode Go route) |

`tools/sync_omp_models.py` imports `~/.omp/agent/models.yml` into the booth profile's
`llm-pi-ai` routes (API keys go to `$DSH_HOME/.env`, never into the profile). Add
`--opencode-default` with `OPENCODE_API_KEY` set to also install the OpenCode Go route and make
`opencode/gpt-6-luna` the default. `^model` mentions use
the same `provider/model[:effort]` selectors as the model picker. On first
start booth writes a profile `cordis.patch.yml` that enables `modelSelection` and `modelDiscovery`
on the base `tool-subagent` entry when the profile has no patch. The kernel change lives in
`libs/dsh/packages/subagent/tool-subagent`: user tags are scanned from durable messages, a
per-Agent system section lists them, and `subagent(agent: "m1")` resolves the tagged provider,
model and effort. `tools/patch_dsh_model_mentions.py` applies the same change to an installed
0.2.1 dsh package (install.sh runs it when Python and dsh are present)

`theme.conf` in the state dir accepts `key=#rrggbb` overrides for `accent`, `text`, `dim`,
`line`, `panel`, `border`, `green`, `yellow`, `red`, `git`, `gray`, `token`, and `hover`. Use
`/theme` to reload the file or `/theme key=#rrggbb` to persist and apply one override live; edits
preserve comments and unrelated lines.

## Credentials

The kernel needs a model credential: dsh reads `DEEPSEEK_API_KEY`, or a key stored through its
Models page. `booth` does not manage credentials. Without one every turn fails at the first
`session/prompt` with the kernel's authentication message, which the console shows above the
composer.

## Limits

- The daemon serves one client at a time (a second connection is refused); a multiplexed
  supervisor is not designed yet.
- `/sessions` resumes dsh's persisted context and replays user, assistant and thinking text from
  dsh's `session.v4` store, including tool calls with input/output/duration. The store is internal
  to dsh and needs the `zstd` CLI; a failure to decompress is shown above the composer.
- The transcript keeps the latest 40 entries; `PageUp`/`PageDown` and the wheel walk the
  rest and `PageDown` at the end returns to the live view. New entries keep the reader's place.
- `keybindings.yml` in the state dir maps `booth.cancel`, `booth.quit`, `booth.editor`,
  `booth.history`, `booth.tools`, `booth.thinking`, `booth.hide_tools`, `booth.paste_image`,
  `booth.reasoning`, `booth.page_up`, `booth.page_down`, `booth.dequeue`, `booth.help`,
  `booth.model`, `booth.sessions`, `booth.copy`, `booth.links`, `booth.model_next` and `booth.retry` to `Ctrl`/`Alt`/`Shift` letter chords, `Esc`,
  `Enter`, `Shift+Tab`, arrows or `PageUp`/`PageDown`. A value may be a comma-separated array
such as `[Alt+H, Ctrl+H]`; `[]` disables that action.
- Mouse hit-testing uses each row's final FTXUI box, so resizing and scrolling keep clicks exact.
- `Ctrl+G` seeds its scratch file through the open descriptor in the first writable of `$TMPDIR`,
  `$XDG_RUNTIME_DIR`, `/tmp` or `/var/tmp`, so a path swap cannot redirect the write.
- Only Linux is tested; the process layer uses `posix_spawn` and pipes; Vim grapheme clusters use
  ICU. The profile patch for subagent model selection is only written when the profile has no
  `cordis.patch.yml`.
- `add-dir`/`remove-dir` require a dsh build with session writable roots; older builds report
  `dirs/unsupported`.
- The control plane is honest about its scope: OMP commands that need a transport or agent
  feature dsh does not mount (relay collaboration, voice, SSH transport, ratchet,
  multi-root workspaces) remain unavailable; browser/computer providers are opt-in installs, and
  the DeepSeek account flow is localhost-only. See
  [OMP_PARITY.md](OMP_PARITY.md). `control.sock` is private to the profile (mode 0600) and one
  supervisor owns it.
