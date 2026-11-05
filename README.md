# dsh-booth

A terminal console for a running [dsh-superagent](https://github.com/deepseek-ai/dsh-superagent-prototype).
Type `booth` and you get one screen: type an instruction and the agent takes it as a ticket,
watch what the executor is doing, and pick commands from a palette that filters as you type —
the frame, palette and composer of `omp`, not a web page.

```text
  ❯ 把 README 里 “booth up” 那句说明改成 “booth up starts the service”
  ↑ sent to the agent  3eb84e6f · picks it up within a second
  ◆ agent · accepted  3eb84e6f
    收到：把 README 示例中 `booth up` 那句说明改成 `booth up` starts the service。
  ◆ agent  3eb84e6f
    PR #1 已把说明改为 “starts the service”，可审阅。

╭─ ◆ booth · deepseek/deepseek-chat · ready · 2/4 · 1q ────────── v0.1.0 ╮
╰─ › 把 README 里 “booth up” 那句说明改成 … ────── Enter run · / commands ╯
```

The composer is one bordered block: the top rail carries the provider, runtime state and
executor load, the bottom rail the input and its hint. The wordmark and a rotating tip sit in
the empty transcript, as they do in `omp`.

## What is in the repository

`superagent/` is a git submodule pinned to one commit of
[dsh-superagent](https://github.com/deepseek-ai/dsh-superagent-prototype). `booth init` builds
it, creates a DSH profile around it and links it in, together with `local-source/`; `booth up`
runs that profile detached and waits for the management API. The console, the service and its deployment therefore ship as one
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
booth send "把 README 里的错字改掉"  # hand one instruction to the agent
booth setup owner/name             # repository + allowed senders, from the shell
booth gh                           # the GitHub CLI the Source would use
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
| `Enter` | Run the line; bare text sends the agent an instruction |
| `Tab` | Complete the selected palette entry |
| `↑` `↓` | Move in the palette, or walk the command history when it is closed |
| `PgUp` `PgDn` | Scroll the transcript (new output snaps back to the bottom) |
| `Esc` | Clear the line and close the palette |
| `Ctrl+L` | Clear the transcript |
| `Ctrl+C` | Quit |

Commands are the same grammar as the one-shot CLI: `/overview`, `/status`, `/tickets`,
`/ticket <id>`, `/threads`, `/thread <key>`, `/decisions`, `/sources`, `/search <text>`,
`/stop`, `/retry`, `/requeue`, `/dismiss`, `/unsubscribe`, `/pause`, `/resume`, plus
`/send`, `/setup`, `/new`, `/model`, `/provider`, `/login`, `/clear` and `/quit`.

## Sending work

dsh-superagent accepts work only from a Source: GitHub, Feishu, or a kind a bundle adds.
Booth ships a third one, `local-source/`, so the console itself is a front door:

- what you type without a slash (or `/send <text>`) is appended to
  `booth-inbox.jsonl` under the state directory;
- the Source reads it as one person's direct message, the Policy admits the sender, and the main
  Session decides the ticket;
- the agent's acknowledgements, replies and notices are appended to `booth-outbox.jsonl`, which
  the console shows as `◆ agent` lines;
- the GitHub and Feishu Sources keep working as before — booth adds a door, it does not replace
  them.

The console follows what it sent:

```text
  ❯ 在 README 里加一句：这一行由 booth 控制台发起
  ↑ sent to the agent  b4e50b13 · picks it up within a second
  ◆ agent · accepted  2457786b
    收到：在 README 里加一句“这一行由 booth 控制台发起”。
  ◇ ticket 2457786b · working
  ◇ ticket 2457786b · needs a person · review
  ◆ agent  2457786b
    [PR #2](https://github.com/owner/repo/pull/2) 已按改后的措辞加上…，可审阅。
```

- **A greeting is not work.** The agent answers `hi` with a reason, not a ticket:
  `◇ agent · not taken  打招呼，没有具体改动请求`. Requests need a concrete change.
- **Follow-ups continue the ticket.** After an accept, the next instruction goes to that
  ticket's thread, so “改成 …” revises the same work; the composer's top rail shows
  `↳ <ticket>`, and the choice survives a console restart. `/new` starts a new ticket.
- **A silent report is not lost.** When the agent did not also send a reply, the console prints
  the ticket's own summary under `◇ ticket … · report`.
- **No decision is still an answer.** After 20 seconds without a judgement the console says so,
  and `/sources` / `/status` explain whether the Source or the service is the reason.

`/setup` names the repository and the people who may ask for work:

```text
  ❯ /setup lanshi17/dsh-booth
✓ repository lanshi17/dsh-booth · agent @lanshi17
✓ console instructions allowed · type an instruction, without a slash
```

It reads the signed-in `gh` account and writes three settings blocks through the management
API: the GitHub Source (repository, CLI, new Issues on), the Policy (that account for GitHub,
`operator` for the console), and the console Source itself. Run inside a checkout, the
repository can be inferred: `/setup`. An optional second argument moves the ticket directories:
`/setup owner/name ~/src/work`. `booth setup owner/name` does the same from a shell.

The bundle's GitHub reads use `gh api --slurp`, which CLIs before 2.57 reject. When the system
`gh` is older, `/setup` downloads a pinned release into `<state>/bin/gh`, checks it against the
published SHA-256 and points the Source at it — the GitHub front door works without touching
the system CLI. `booth gh` reports the choice; `booth gh --install` installs it on demand.

The concurrency limit, the worker model and the prompts stay where they were: `/model` and the
web panel.

## When nothing seems to happen

| What you see | What it means |
| --- | --- |
| `◇ agent · not taken  …` | The agent judged the instruction and declined; its sentence says why (a greeting, no concrete change, an unrelated request) |
| `⋯ no decision yet` | The Source has not produced a judgement after 20 s; `/sources` shows whether it is polling and `/status` shows health |
| `◇ ticket … · needs a person` | The worker is waiting for an answer; `/ticket <id>` shows the question, and a follow-up message answers it |
| `↑ sent …` and no further line | The console wrote the instruction but the service is down or still starting; `booth status`, `booth logs` |

The web panel at the same port is the same service; anything the console cannot edit (policy
rules, prompts, concurrency) is there.

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
- **Two-second poll.** The header refreshes on start and after each command, and a light timer
  rereads `/state` and the outbox every two seconds so acknowledgements and replies arrive
  while you watch. A future `booth watch` can push instead.
- **Work enters through a Source, never a hidden route.** The management API has no "submit a
  request" route, and booth does not invent one: `local-source/` is a real Source kind, loaded
  by the profile next to `dsh-superagent/github-watch`, so instructions pass the same Policy
  gate and Eval as a GitHub issue. Removing that bundle removes the console's door, not its
  reads.

## Compatibility

The submodule pins `dsh-superagent` `0.1.0` (`@deepseek-ai/dsh` `0.2.1-*`); the console was
built and tested against that commit. The console needs the service's loopback port and the
routes listed above. If a route moves, `api.rs` is the only file to change; the wire types in
`model.rs` are the second.

Not implemented: live push (SSE/WebSocket), editing prompts or the policy rules from the
console, Feishu-specific reads, attaching to a worker Session, Windows service control.

## License

GPL-3.0-only. `dsh-superagent`, which this console talks to, is MIT and remains a research
prototype; this project is not an official DeepSeek product.