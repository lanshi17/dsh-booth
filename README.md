# dsh-booth

A terminal console for a running [dsh-superagent](https://github.com/deepseek-ai/dsh-superagent-prototype).
Type `booth` and you get one screen: what is waiting for a person, what the executor is doing,
and a command palette that filters as you type — the entry shape of `omp` and `dst`, not a
web page.

```text
booth · http://127.0.0.1:3082 · ready · 2/4 running · 1 queued
booth — a terminal console for dsh-superagent
type / for commands, or a word to search. Enter runs, Tab completes.

> /tickets
tickets (open) · 3 total
person  t-7f3a9c2e Fix the flaky retry test  11-04 10:12
           waiting for your answer on the design
working t-91c4e07a Add a --dry-run flag        11-04 10:08

booth>
Enter run · Tab complete · ↑↓ pick/history · PgUp/PgDn scroll · Esc clear · Ctrl+C quit
```

## Why this is a separate repository

It depends on dsh-superagent; dsh-superagent does not depend on it. The console talks only
to the bundle's loopback management API (`/orchestrator/api/*`), so it adds no plugin to the
bundle, changes no DSH profile, and survives a DSH upgrade as long as the routes do.

The API is guarded by `sameOrigin()` in `src/app/http.ts`: the `Host` header must be
`127.0.0.1:<port>` or `localhost:<port>`, and every request carries
`x-orchestrator-request: 1`. A local process satisfies both; no token is involved.

## Install

```sh
cargo install --path .          # installs `booth`
# or
cargo build --release           # ./target/release/booth
```

The service must already run — see the bundle's `docs/006-deployment.md`:

```sh
scripts/dsh-superagent start
```

## Try it without a service

```sh
python3 dev/mock-api.py 3899 &
booth --url http://127.0.0.1:3899
```

The fixture serves the same routes with the same same-origin guard, so the console can be
developed and reviewed without a configured GitHub account.

## Usage

```sh
booth                              # the console
booth status                       # one shot: health, pause, setup, accounts
booth overview                     # problems, people waiting, executor load, 24h flow
booth tickets --state person       # open | person | working | idle | ended
booth ticket t-7f3a9c2e1b          # mandate, basis, assignments, inputs, threads
booth threads --filter recent      # recent | subscribed | evaluating | all
booth thread 'github:owner/repo#12'
booth decisions
booth sources
booth search retry
booth stop t-7f3a9c2e1b            # withdraw the mandate
booth retry t-7f3a9c2e1b           # start a fresh assignment
booth pause                        # / booth resume
```

Every read command takes `--json` and prints the view as served, for scripts and the hourly
check the deployment guide asks for.

| Global flag | Default | Meaning |
| --- | --- | --- |
| `--url` | `http://127.0.0.1:3082` | The management API's base URL |
| `DSH_SUPERAGENT_URL` | — | Same value from the environment |

## Inside the console

| Key | Does |
| --- | --- |
| `Enter` | Run the line; bare text searches tickets, threads and triggers |
| `Tab` | Complete the selected palette entry |
| `↑` `↓` | Move in the palette, or walk the command history when it is closed |
| `PgUp` `PgDn` | Scroll the transcript (new output snaps back to the bottom) |
| `Esc` | Clear the line and close the palette |
| `Ctrl+L` | Clear the transcript |
| `Ctrl+C` | Quit |

Commands are the same grammar as the one-shot CLI: `/overview`, `/status`, `/tickets`,
`/ticket <id>`, `/threads`, `/thread <key>`, `/decisions`, `/sources`, `/search <text>`,
`/stop`, `/retry`, `/requeue`, `/dismiss`, `/unsubscribe`, `/pause`, `/resume`, `/clear`,
`/quit`.

## Design notes

- **One grammar, two front ends.** `commands::execute` returns transcript lines; the TUI
  appends them, `booth <command>` prints them. The palette, `/help` and the CLI read the same
  entry table.
- **Decoding is tolerant on purpose.** The bundle is a research prototype whose views grow;
  fields this console does not render stay `serde_json::Value`, and unknown fields are
  ignored. A new view field does not break the console, only a renamed route does.
- **No polling loop.** The header refreshes on start and after each command; the service is
  not asked for state every second. A future `booth watch` can change that without touching the
  command layer.
- **It does not create work.** The API exposes stop, retry, requeue, dismiss and pause, but
  no "submit a request" route. `booth` deliberately stays read-and-control; asking for new work
  goes through GitHub or Feishu as a person would.

## Compatibility

Built and tested against the management API of `dsh-superagent` `0.1.0`
(`@deepseek-ai/dsh` `0.2.1-*`). The console needs the service's loopback port and the
routes listed above. If a route moves, `api.rs` is the only file to change; the wire types in
`model.rs` are the second.

Not implemented: work submission, live push (SSE/WebSocket), settings or prompt editing,
Feishu-specific reads, attaching to a worker Session.

## License

GPL-3.0-only. `dsh-superagent`, which this console talks to, is MIT and remains a research
prototype; this project is not an official DeepSeek product.