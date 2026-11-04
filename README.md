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

## What is in the repository

`superagent/` is a git submodule pinned to one commit of
[dsh-superagent](https://github.com/deepseek-ai/dsh-superagent-prototype). `booth init` builds
it, creates a DSH profile around it and links it in; `booth up` runs that profile detached and
waits for the management API. The console, the service and its deployment therefore ship as one
entry point — while the bundle stays an independent MIT-licensed project that knows nothing
about booth.

The console itself talks only to the bundle's loopback management API (`/orchestrator/api/*`),
so a DSH upgrade does not break it as long as the routes do.

The API is guarded by `sameOrigin()` in `src/app/http.ts`: the `Host` header must be
`127.0.0.1:<port>` or `localhost:<port>`, and every request carries
`x-orchestrator-request: 1`. A local process satisfies both; no token is involved.

## Install

```sh
git clone --recursive git@github.com:lanshi17/dsh-booth.git
cd dsh-booth
cargo install --path .          # installs `booth`; or cargo build --release
booth init                      # pnpm install + build, profile `superagent`, link
booth up                        # dsh --profile superagent --no-open
booth                           # the console
```

`booth init` needs `dsh`, `pnpm` and Node. It skips what is already done, so it is safe to
rerun. Clone with `--recursive`, or run `git submodule update --init` — otherwise `superagent/`
is empty.

Updating the bundle:

```sh
git submodule update --remote superagent
booth init        # rebuild
booth restart
```

## Try it without a service

```sh
python3 dev/mock-api.py 3899 &
booth --url http://127.0.0.1:3899
```

The fixture serves the same routes with the same same-origin guard, so the console can be
developed and reviewed without a configured GitHub account or a running service.

## Usage

```sh
booth                              # the console
booth init                         # prepare the profile and build (once)
booth up                           # start the service in the background
booth down                         # stop the service booth started
booth restart
booth logs --lines 40
booth provider                     # the model provider `up` starts with (no secret)
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
| `BOOTH_PORT` | `3082` | Port written into the profile and used by `up` |
| `BOOTH_PROFILE` | `superagent` | DSH profile name |
| `BOOTH_STATE_DIR` | `$DSH_HOME/state/superagent` | Database, `booth.pid`, `booth.log` |
| `BOOTH_SUPERAGENT_DIR` | the repository's `superagent/` | Checkout to build and link |
| `DSH_HOME` | `~/.dsh` | Where profiles and state live |

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
`/stop`, `/retry`, `/requeue`, `/dismiss`, `/unsubscribe`, `/pause`, `/resume`, plus
`/model`, `/login`, `/clear` and `/quit`.

## Providers

`/login` and `/model` follow Pi's shape.

`/login` starts with the DeepSeek account, then offers DeepSeek, OpenAI, OpenRouter or a custom
OpenAI-compatible endpoint with the key behind a masked field; every field stays editable
(`preset`, `id`, `base`, `model`, `api`, `key`).

The account option runs the same PKCE flow as DSH's web panel: booth asks
`platform.deepseek.com` for an authorize URL, listens on `127.0.0.1:37029/oauth/callback` for
the redirect, and exchanges the code for the grant DSH's `deepseek-official` provider consumes.
If the browser cannot reach the loopback port, paste the callback URL from the address bar into
the same window and press Enter — the state is checked before the exchange. `BOOTH_OAUTH_PORT`
picks another port. `/model` lists the providers already stored
and switches the default model, or jumps to `/login` when there are none. Both save and
restart the service in one step — no file editing, no second command.

What lands on disk:

- `provider.json` (mode 0600) under the state directory holds the keys;
- `provider.yml` is a DSH patch overlay with `llm-pi-ai` and `agent-default-model`;
- `booth up` passes `--patch provider.yml` and exports `BOOTH_KEY_<ID>` for each provider.

`booth provider` prints what the service will use, never the secret. Deleting those two files
falls back to the profile's own default model.

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

The submodule pins `dsh-superagent` `0.1.0` (`@deepseek-ai/dsh` `0.2.1-*`); the console was
built and tested against that commit. The console needs the service's loopback port and the
routes listed above. If a route moves, `api.rs` is the only file to change; the wire types in
`model.rs` are the second.

Not implemented: work submission, live push (SSE/WebSocket), settings or prompt editing,
Feishu-specific reads, attaching to a worker Session, Windows service control.

## License

GPL-3.0-only. `dsh-superagent`, which this console talks to, is MIT and remains a research
prototype; this project is not an official DeepSeek product.