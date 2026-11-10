# dsh-booth

A terminal console for a running [dsh-superagent](https://github.com/deepseek-ai/dsh-superagent-prototype).
Type `booth` and you get one screen: type an instruction and the agent takes it as a ticket,
watch what the executor is doing, and pick commands from a palette that filters as you type —
the frame, palette and composer of `omp`, not a web page. The top rail carries the provider,
the executor load, how many tickets are open, how many want a person, and the repository; above
the composer a braille spinner and elapsed time say what the console is waiting on.

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
booth provider                     # the model catalog the profile reports (no secret)
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

| Global flag | Default | Meaning |
| --- | --- | --- |
| `--url` | `http://127.0.0.1:3082` | The management API's base URL |
| `DSH_SUPERAGENT_URL` | — | Same value from the environment |
| `BOOTH_PORT` | `3082` | Port written into the profile and used by `up` |
| `BOOTH_PROFILE` | `superagent` | DSH profile name |
| `BOOTH_STATE_DIR` | `$DSH_HOME/state/superagent` | Database, `booth.pid`, `booth.log` |
| `BOOTH_SUPERAGENT_DIR` | the repository's `superagent/` | Checkout to build and link |
| `DSH_HOME` | `~/.dsh` | Where profiles and state live |

Every read command takes `--json` and prints the view as served, for scripts and the hourly
check the deployment guide asks for.

## Inside the console

| Key | Does |
| --- | --- |
| `Enter` | Run the line; bare text sends the agent an instruction |
| `Ctrl+P` | Open the command palette on an empty line |
| `Ctrl+R` | Search the history (fuzzy, newest first; it survives restarts) |
| `Ctrl+A` `Ctrl+E` | Start / end of the line |
| `Alt+B` `Alt+F` | Move by word |
| `Ctrl+W` | Delete the word before |
| `Ctrl+U` `Ctrl+K` | Delete to the start / end, kept for `Ctrl+Y` |
| `Ctrl+Y` | Paste the last deleted text |
| `Tab` | Complete the selected palette entry |
| `↑` `↓` | Move in the palette, or walk the command history when it is closed |
| `PgUp` `PgDn` | Scroll the transcript (new output snaps back to the bottom) |
| `Esc` | Clear the line and close the palette |
| `Ctrl+L` | Clear the transcript |
| `Ctrl+C` | Quit |

Commands the one-shot CLI also has keep the same names: `/overview`, `/status`, `/tickets`,
`/ticket <id>`, `/threads`, `/thread <key>`, `/decisions`, `/sources`, `/search <text>`,
`/stop`, `/retry`, `/pause`, `/resume`, `/send`, `/setup`, `/agents`, `/eval` and `/provider`.
The console adds `/requeue`, `/dismiss`, `/unsubscribe`, `/new`, `/model`, `/login`, `/keys`,
`/history`, `/refresh`, `/clear` and `/quit`.

The palette is fuzzy and scrolls: `/ti` ranks `/tickets` and `/ticket`, matched letters are
highlighted, and a selection past the eighth entry scrolls the window the way `omp`'s lists do.
`/keys` lists every shortcut on one screen.

## What the console keeps

dsh-superagent's arrangement is the project's point, and the console reports it rather than
replacing it. `/agents` (or `booth agents`) prints the whole picture:

| Property | Where it lives |
| --- | --- |
| One agent routes, another works | The main Session (`dsh-superagent-router`) judges each Trigger; every Ticket gets its own `worker-<ticket>` Session (`dsh-superagent-worker`) |
| Several agents in parallel | `maxWorkers` (3 by default) runs Tickets side by side, plus one extra slot so a person's message never waits for a free worker |
| Memory that outlives a conversation | Sessions persist under `<dsh_home>/sessions/`; the router keeps the tickets, threads and decisions it judged, and a worker keeps its conversation when a later message resumes its Ticket. Restarting the console loses nothing |
| Deciding alone, asking rarely | A Ticket reaches `person` only for an answer or a review, and `/overview` lists exactly those; workers run unattended otherwise |
| Work that starts by itself | GitHub Issues, comments and CI, Feishu messages and console instructions all enter the same way, so nothing has to be typed |

`/eval` (or `booth eval`) decides how much of that starts without a person:

| Mode | What becomes a ticket on its own |
| --- | --- |
| `off` | Only an explicit request from someone on the list |
| `observe` (default) | Nothing; inferred candidates are recorded with what the agent would have done |
| `auto` | Inferred candidates too, when an enabled policy rule matches |

Changing the provider does not disturb any of it. `/login` and `/model` register every stored
provider in the overlay and only change which one is the default, so a Session created earlier
keeps its adapter and its history.

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
  and `/sources` / `/status` explain whether the Source or the service is the reason. It keeps
  watching for ten minutes — a judgement can wait behind another turn — and a late one still
  appears in place.

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

A session keeps the provider it was created with, so the overlay always registers **every** stored
provider: signing in drops only the default model, which makes the DeepSeek account the default
while sessions that still name an API-key provider keep their adapter and their history. Nothing
is discarded on sign-in or on `/model`. `booth reset-sessions` moves the orchestrator's session
and every ticket's worker session aside if one ends up naming a provider that is genuinely gone.

The concurrency limit, the worker model and the prompts stay where they were: `/model` and the
web panel.

## When nothing seems to happen

| What you see | What it means |
| --- | --- |
| `◇ agent · not taken  …` | The agent judged the instruction and declined; its sentence says why (a greeting, no concrete change, an unrelated request) |
| `⋯ no decision yet` | The Source has not produced a judgement after 20 s; `/sources` shows whether it is polling and `/status` shows health |
| `◇ ticket … · needs a person` | The worker is waiting for an answer; `/ticket <id>` shows the question, and a follow-up message answers it |
| `↑ sent …` and no further line | The console wrote the instruction but the service is down or still starting; `booth status`, `booth logs` |
| `◇ agent · could not judge  timeout` | The assessment ran past the Eval timeout; the panel's Eval block raises `timeoutSeconds`, and `/requeue <id>` puts the instruction back |
| `no adapter registered for provider …` in a session | The profile layer no longer registers that provider; `/login` adds it back, or `booth reset-sessions` starts fresh sessions — the second costs the conversation history, so prefer the first |

The web panel at the same port is the same service; anything the console cannot edit (policy
rules, prompts, concurrency) is there.

## Providers

**The providers are the profile's.** booth keeps no provider list of its own: what `/provider`
and the header show is the service's `GET /models` answer — the same catalog the web panel
renders — and a change is written into the layer DSH already composes
(`~/.dsh/profiles/<name>/cordis.patch.yml`). So a provider added in the panel appears in booth,
one added in booth appears in the panel, and `booth up` passes no `--patch` and no extra
environment.

`/model` lists the catalog: provider, model id and its display name, with the current one
marked and an **add provider** row at the end. Choosing one writes `agent-default-model` into
the profile layer and restarts the service.

`/login` follows Pi's shape. The first entry is the DeepSeek account; the others are
API-key endpoints (DeepSeek, OpenAI, OpenRouter, or any OpenAI-compatible base URL), with every
field editable. An API-key login writes:

- an `llm-pi-ai.providers.<id>` entry in the profile layer — `apiKeyEnv`, `baseURL`, `api` and
  the model — naming a credential ref, never the secret;
- the key itself as `refs.BOOTH_<ID>_KEY` in `~/.dsh/.credentials.yaml` (mode 0600), beside the
  keys DSH already stores there;
- `agent-default-model`, so the next start uses it.

The account option runs the same PKCE flow as DSH's web panel: booth asks
`platform.deepseek.com` for an authorize URL, listens on `127.0.0.1:37029/oauth/callback` for
the redirect, and exchanges the code for the grant DSH's `deepseek-official` provider consumes.
Signing in **removes** booth's `agent-default-model` entry, so the account is the default again
by the profile's own rule rather than by an override. The authorize URL wraps in full instead of
being cut, and is also written unwrapped to `oauth-url.txt` under the state directory:
**Ctrl+O** opens it in a browser, **Ctrl+Y** copies it. If the browser cannot reach the loopback
port, paste the callback URL into the same window and press Enter — quotes, a `#fragment` and a
bare `code=…&state=…` are accepted. `BOOTH_OAUTH_PORT` picks another port.

A machine that ran an earlier booth is migrated once, on `booth up`: `provider.json` is turned
into the profile entries and credential refs above, keeping the provider it marked `current` as
the default, then renamed to `provider.json.migrated` and the stale `provider.yml` is deleted.
`booth provider` prints the shared view and never a secret.

## Design notes

- **One source for models.** booth reads the catalog the service reports and writes changes into
  the profile layer DSH composes, so there is no second configuration to keep in step: no
  `provider.json`, no generated `--patch` overlay, no injected key variables.
- **The console borrows `omp`'s habits, not just its colours.** Palette entries are fuzzy and
  the picker keeps the selection in view; the editor has readline motion and a kill ring
  (`Ctrl+W`, `Ctrl+U`, `Ctrl+K`, `Ctrl+Y`, `Alt+B`/`F`); `Ctrl+R` searches history; `Ctrl+P`
  opens the palette; the working row is a braille spinner, elapsed time and one label; `/keys`
  lists the lot.

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

The console and `dsh-superagent` versions are those the submodule pins. The console depends
only on the management API and does not modify `dsh-superagent`'s source.

## Who does the work

The default worker is dsh. Name another coding agent when you want that one to do it:

```sh
booth send "fix the readme"            # dsh
booth send --agent omp "fix the readme"
booth send --agent claude "fix the readme"
booth send --agent codex "fix the readme"
booth send --agent cursor-agent "fix the readme"
```

In the console, start the line with `@omp`, `@claude`, `@codex`, or `@cursor-agent`. A line with no prefix goes to dsh. The named agent runs the instruction itself. Booth does not keep a second control plane.

## Decision

The portal does not pick among its own options, and a sub-agent does not bring an open choice to the portal first. Both hand the choice to `booth decide`. The call is a [System One](https://commandcode.ai/docs/provider#decision-models-typesafe-jev) request for `typesafe/jev`: one `state`, and up to 100 `questions`. A question is `noul`, `choice`, or `score`. `from` is `subagent` or `portal` and stays with booth.

A choice or score at confidence 0.7 or more is the route, and so is a noul that is clearly yes or no. Below that, or on an error, a sub-agent's call escalates to the portal. The portal calls `booth decide` again with `"from": "portal"`. If that call also cannot route, it escalates to the user. `booth up` installs this order into the team's prompt.

## License

GPL-3.0-only. `dsh-superagent`, which this console talks to, is MIT and remains a research
prototype; this project is not an official DeepSeek product.