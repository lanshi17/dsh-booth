# booth

A terminal console for one dsh profile. `booth` starts `dsh --profile booth` and drives it over
ACP, showing the conversation and streaming the agent's work. There is no second agent layer and
no router: dsh is the agent, booth is its front end.

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

The TUI follows omp's shape: it runs inline (no alternate screen), prints the splash once, and
keeps the transcript above a two-row composer. The composer's top border is a dark chip band with
the model, reasoning effort when dsh reports it, cwd, git branch/dirty counts, working timer and
context usage; the bottom row merges
the prompt with the key hints. User turns keep omp's full-width band, assistant turns render
markdown (lists, links, tables, inline code, inline LaTeX math, syntax-highlighted fences), and tool calls render as
full-width frames with input, output and duration. `/copy` puts the last answer on the terminal
clipboard with OSC 52 (tmux passthrough included); `/export [path]` writes the full transcript as Markdown (default:
`$BOOTH_STATE_DIR/transcript-<timestamp>.md`). `Enter` sends, `Alt+Enter` inserts a newline, `Tab` completes the word at the caret from prompt history and committed user/assistant transcript text (unless a command, file, or model-mention menu is open), `ArrowUp`/`ArrowDown` walk the prompt history,
`Ctrl+C` cancels the run, `Ctrl+D` quits and saves the unsent draft for the next start, `Ctrl+R`
searches prompt history, `Ctrl+G`
opens `$VISUAL`/`$EDITOR` on the prompt (`booth.editor` in `keybindings.yml` moves that key),
`PageUp`/`PageDown` or the wheel browse the transcript, `Shift+Tab` cycles dsh's reasoning
effort. The editor supports `Ctrl+Z` undo, `Alt+Z` redo, `Ctrl`/`Alt`+`←`/`→` word movement,
`!command` runs a local shell command during any turn and `$python` runs Python locally (an
expression prints its value); neither is sent to the model or saved as prompt history.
`Ctrl+W`/`Ctrl+U`/`Ctrl+K` kill and `Ctrl+Y` yank, and `Alt+Up` takes a queued prompt back into
the editor. Drafts taller than eight rows scroll internally with a `↑N` marker; a permission card accepts
`y` (once), `a` (always), `n` (deny once), `d` (deny always).
Commands: `/status`, `/model`, `/sessions`, `/new`, `/close`, `/copy`, `/clear`, `/retry`,
`/export`, `/effort`, `/theme`, `/notify`, `/links`, `/mouse`, `/context`, `/stats`, `/queue`, `/session`, `/acp`, `/cancel`,
`/find`, `/attach`, `/detach`, `/help`, `/quit`. `/notify` reports the setting; `/notify on` and
`/notify off` persist terminal notifications in `$BOOTH_STATE_DIR/notify`. Notifications default to
on and use OSC 9 for permission requests and turns lasting at least five seconds (tmux passthrough
is supported).
Aliases: `/resume`, `/models`, `/reasoning`, `/hotkeys`, `/keys`, `/exit`, `/q`.
`/copy`, `/clear`, `/export`, `/status`, `/context`, `/stats`, `/effort`, `/theme`, `/queue`,
`/session`, `/acp`, `/cancel`, `/notify`, `/links`, `/mouse`, `/find`, `/attach`, `/detach`, `/help` and `/quit` stay available
while a turn is running; a plain prompt typed during a run is queued and sent when the run ends.
`/model` opens dsh's model list grouped by provider (↑/↓ or wheel, Enter); the selection is applied
through the ACP config option. `/effort` changes dsh's reasoning effort: `next` cycles, a level
name or wire value selects it, and no argument lists the current value and choices. `/theme`
reloads `$BOOTH_STATE_DIR/theme.conf` into the running TUI; `/theme key=#rrggbb` validates,
persists, and applies one color override immediately. Supported keys are `accent`, `text`, `dim`,
`line`, `panel`, `border`, `green`, `yellow`, `red`, `git`, `gray`, `token`, and `hover`. Invalid
keys or colors leave the file unchanged and show a notice.
`/context` reports dsh's context window usage; `/stats` counts local transcript entries, tools and
queued prompts. `/new` closes the current session and leaves a clean transcript, while `/clear`
only clears the local transcript (dsh has no ACP call that clears a live session's context).
`/queue <text>` enqueues a prompt for when the active turn ends; `Alt+Up` brings the newest
queued prompt back into the editor. `/session` prints the live ACP session (id, title, model,
effort, context, queue), and `/acp` prints the protocol, phase, connection and socket.
`/attach <path>` adds an ACP `resource_link` to the next prompt (shown as a `file N` status chip);
`/detach [name|all]` clears pending links, and `/cancel` is the explicit ACP cancel command.
`/find <text>` searches the local transcript and cycles older matches on repeated calls.
`/links` (or `Alt+L`) lists openable transcript URLs with `a`–`z`/`A`–`Z` hint keys; Enter or a
hint opens the selection and Esc closes. `/mouse` reports capture; `/mouse on|off` persists it.
Capture defaults off inside tmux so wheel scrollback and native/tmux selection copy work; turn it
on for booth's wheel scrolling, clicks and hover. `/copy` still writes through tmux's OSC 52
passthrough. `/help` (or `Alt+H`) lists every command and alias; the card scrolls with ↑/↓,
PageUp/PageDown, Home/End and the wheel, and Esc or `q` closes it.
A prompt starting with `!` runs the rest as a local `/bin/sh -c` command in the working directory,
shows a `bash` tool card, and keeps the command/output out of model context and prompt history.
`Ctrl+R` searches prompt history; `Ctrl+O` expands tool output, `Ctrl+T` hides or shows thinking,
and `Alt+O` (or `Ctrl+Shift+O` on terminals that report it) hides or shows tool cards. Clicking a
tool row toggles tool expansion. Mouse clicks also select and activate slash, file, model,
session, history, copy and model-mention rows, place the composer caret, open OSC-8 links, answer
permission buttons and dismiss help; moving the pointer paints a hover band. `Ctrl+V` attaches a clipboard raster to the next prompt
(shown as an `image N` chip); `:smile:`-style shortcodes expand when the prompt is sent. Set
`BOOTH_VIM=1` for OMP-parity modal editing: insert/normal/visual/visual-line/replace, counts,
`h j k l 0 ^ $ w b e f F t T ; , gg G % { }`, `d/y/c/>/<`, `dd/yy/cc`, `D/C/x`, `p/P`, `u`,
`.`, `J`, `r/R`, text objects (`iw aw iW aW`, quotes, brackets, `ip/ap`), desired column and
sticky `$`. Type `^` anywhere in the prompt for
model autocomplete; known mentions become stable `mN` tags carrying their route. The patched dsh
`subagent` tool lists those aliases in the system prompt and accepts `agent: "m1"`, resolving the
route in the kernel; aliases persist in the conversation tags and rebuild on resume. `/sessions` lists saved sessions: Enter resumes, `r` refreshes, `c` closes the active
session. Type `/` for the fuzzy command menu below the input: names, aliases and descriptions all
match. ↑/↓ or the wheel moves, PgUp/PgDn/Home/End jump, Tab completes the selected command, Enter runs the selection
and Esc dismisses the menu without clearing the draft. The wheel also moves the model and session
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
- The inline transcript keeps the latest 40 entries; `PageUp`/`PageDown` and the wheel walk the
  rest and `PageDown` at the end returns to the live view. New entries keep the reader's place.
- `keybindings.yml` in the state dir maps `booth.cancel`, `booth.quit`, `booth.editor`,
  `booth.history`, `booth.tools`, `booth.thinking`, `booth.hide_tools`, `booth.paste_image`,
  `booth.reasoning`, `booth.page_up`, `booth.page_down`, `booth.dequeue`, `booth.help`,
  `booth.model`, `booth.sessions`, `booth.copy` and `booth.links` to `Ctrl`/`Alt`/`Shift` letter chords, `Esc`,
  `Enter`, `Shift+Tab`, arrows or `PageUp`/`PageDown`.
- Mouse hit-testing uses each row's final FTXUI box, so resizing and scrolling keep clicks exact.
- Only Linux is tested; the process layer uses `posix_spawn` and pipes; Vim grapheme clusters use
  ICU. The profile patch for subagent model selection is only written when the profile has no
  `cordis.patch.yml`.
- OMP commands that manage its own runtime (`/compact`, `/login`, `/mcp`, `/plugins`,
  `/marketplace`, `/collab`, `/agents`, `/jobs`, `/usage`, `/tools`, `/git`) have no dsh ACP
  equivalent and are not emulated.
