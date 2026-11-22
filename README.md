# booth

A terminal console for one dsh profile. `booth` starts `dsh --profile booth` and drives it over
ACP, showing the conversation and streaming the agent's work. There is no second agent layer and
no router: dsh is the agent, booth is its front end.

## Build

Requirements: CMake 3.28+, Ninja, a C++23 compiler (GCC 15+), and network access for the pinned
FetchContent dependencies (CLI11, nlohmann/json, Catch2, FTXUI).

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
markdown (lists, links, tables, inline code, syntax-highlighted fences), and tool calls render as
full-width frames with input, output and duration. `/copy` puts the last answer on the terminal
clipboard with OSC 52 (tmux passthrough included); `/export [path]` writes the full transcript as Markdown (default:
`$BOOTH_STATE_DIR/transcript-<timestamp>.md`). `Enter` sends, `Alt+Enter` inserts a newline, `ArrowUp`/`ArrowDown` walk the prompt history,
`Ctrl+C` cancels the run, `Ctrl+D` quits, `Ctrl+G` opens `$VISUAL`/`$EDITOR` on the prompt
(`booth.editor` in `keybindings.yml` moves that key), `PageUp`/`PageDown` or the wheel browse the transcript, `Shift+Tab` cycles dsh's reasoning effort; a permission card accepts
`y` (once), `a` (always), `n` (deny once), `d` (deny always).
Commands: `/status`, `/model`, `/sessions`, `/close`, `/copy`, `/clear`, `/retry`, `/export`,
`/help`, `/quit` (`/exit` is an alias). `/copy`, `/clear`, `/export`, `/status`, `/help` and `/quit` stay available while a turn
is running; a
plain prompt typed during a run is queued and sent when the run ends.
`/model` opens dsh's model list grouped by provider (↑/↓ or wheel, Enter); the selection is applied
through the ACP config option. `/sessions` lists saved sessions: Enter resumes, `r` refreshes, `c` closes the active
session. Type `/` for the fuzzy command menu below the input: names, aliases and descriptions all
match. ↑/↓ or the wheel moves, PgUp/PgDn/Home/End jump, Tab completes, Enter runs the selection
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
- `keybindings.yml` in the state dir maps `booth.cancel`, `booth.quit` and `booth.editor`;
  paging and copy use fixed keys.
- Only Linux is tested; the process layer uses `posix_spawn` and pipes.
