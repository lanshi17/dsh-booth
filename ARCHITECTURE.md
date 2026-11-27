# Architecture

`booth` is an ACP client for one dsh profile. It starts the kernel, renders its events, and turns
keystrokes into prompts, cancels and permission answers.

```
 booth CLI ──┬── run ───────────────┐
            ├── supervisor ────────┤
            └── daemon (socket) ───┤
                                  ▼
                          Supervisor (posix_spawn)
                                  │ stdio JSON-RPC
                                  ▼
                          dsh --profile booth
                            │            │
                      ACP events    control.sock
                            │            │
                            ▼            ▼
                     projection → AppState → FTXUI
```

The ACP pipe carries the standard automation surface. A booth-owned profile
plugin (`booth-control.mjs`) lives in the same dsh process and opens a private
Unix socket for operations ACP does not expose: native slash commands,
session rename/delete/fork, event-tree reads, the live agent roster, jobs and
the visible tool list. Requests and replies are newline-delimited JSON; the
client serializes them on the worker thread.

## Layers

### CLI

`booth run` starts an in-process supervisor, opens a session, sends one prompt and exits.
`booth up` detaches the daemon; `run` and the TUI then connect to its Unix socket and forward
ACP traffic. `sessions`, `status` and `logs` read from the kernel or the state dir. The profile
package and its overlay file are created by the supervisor's config layer on first start.

### Supervisor

One process per state dir, guarded by `flock` and a pid file. It spawns `dsh --profile booth`
with `posix_spawn`, redirects its stderr to `booth.log`, performs the ACP `initialize` handshake
and keeps stdin/stdout as the protocol pipe. `down` closes the pipes, terminates the child and
removes the pid file. The daemon owns one supervisor and accepts one client at a time.

### ACP client

`src/subagent/acp` frames JSON-RPC lines, correlates requests with timeouts, and maps
`session/update` payloads into events: stream deltas, transcript entries, permission requests,
config changes and usage. Non-JSON stdout lines from kernel plugins are ignored, not treated as
protocol errors. The client never blocks the UI thread; a reader thread pushes events into the
inbox.

### Control plane

`src/control/client.cpp` connects to `control.sock` and correlates one request
at a time. The supervisor installs `booth-control.mjs` from the binary into the
profile and keeps one managed insert row in `cordis.patch.yml`; the plugin
injects `agents`, `commands`, `goals`, `jobs`, `planMode`, persistence, query,
sandbox policy, session title and tools. It also relocates sessions (`/move`, `/wt`), manages
session writable roots where the harness exposes them, and serves the loop/preset state the TUI
keeps on the main thread. Remote failures carry their code on the transcript.
`src/interaction/control_commands.cpp` maps booth commands to that protocol;
`git_commands.cpp` and `extra_commands.cpp` own local git and transcript/mcp
commands.

### Interaction

`src/interaction/state.cpp` is the only mutation point: it applies one `Event` to `AppState`.
`view.cpp` renders that state as FTXUI elements; `scroll.cpp` windows the transcript and keeps a
reader's place when new entries arrive; `tui.cpp` owns the component tree, input, history,
commands and the worker that sends ACP requests. The TUI renders full-screen in the alternate
screen by default; `BOOTH_INLINE=1` switches to FTXUI's inline `TerminalOutput` mode, where the
splash is printed once into the scrollback. Either way the component is the transcript plus omp's
two-row composer (status chips on the top border, input merged into the bottom border).

### Profile

The `booth` profile is a real dsh profile (`$DSH_HOME/profiles/booth/package.json`) with the
`@deepseek-ai/dsh-base` and `@deepseek-ai/dsh-acp-app` bundles. It is created on first start.
`booth` starts `dsh --profile booth` with no extra loader patch.

## Invariants

1. Views never mutate state; only `apply(Event)` does.
2. The UI thread never waits on the kernel, the control socket, a child process, or a worker lock.
3. Every wait has a visible state; failures are one `Problem` naming its layer.
4. stdout of the kernel is protocol only; log lines are skipped.
5. One state dir has one supervisor; the lock and pid file enforce it.

## Verification

- Unit tests cover the codec, ACP projection, state reducer, keymap, process layer, profile
  writer and the skill catalog plus slash ranking.
- PTY tests drive the real TUI with a fake ACP kernel and a fake control socket: start, prompt,
  permission, cancel, quit, export, external editing, transcript paging, the daemon path, and the
  control lifecycle (rename, compact, tools, jobs, agents, tree, fork, delete).
- `tools/gate.sh` runs the build, tests and the 600-line module limit.
