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
                                  │
                              ACP events
                                  ▼
                          projection → AppState → FTXUI
```

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
2. The UI thread never waits on the kernel, a child process, or a worker lock.
3. Every wait has a visible state; failures are one `Problem` naming its layer.
4. stdout of the kernel is protocol only; log lines are skipped.
5. One state dir has one supervisor; the lock and pid file enforce it.

## Verification

- Unit tests cover the codec, ACP projection, state reducer, keymap, process layer and profile
  writer.
- PTY tests drive the real TUI with a fake ACP kernel: start, prompt, permission, cancel, quit,
  export, external editing, transcript paging and the daemon path.
- `tools/gate.sh` runs the build, tests and the 600-line module limit.
