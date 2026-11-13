# dsh-booth

A terminal console for a running [dsh-superagent](https://github.com/deepseek-ai/dsh-superagent-prototype).
Type `booth` and talk to the portal agent, which leads the team and delegates work to workers,
watch what the executor is doing, and pick commands from a palette that filters as you type —
the frame, palette and composer of `omp`, not a web page. The top rail carries the provider,
the executor load, how many tickets are open, how many want a person, and the repository; above
the composer a braille spinner and elapsed time say what the console is waiting on.

```text
  ❯ 检查 README 的安装说明并修正过时的命令
  ↑ sent to the portal  3eb84e6f
  ◆ agent · 我会核对安装命令，再让 worker 修改 README 并汇总结果。

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
entry point — while `superagent/` remains an MIT-licensed bundle with its own runtime.

The console itself talks only to the bundle's loopback management API (`/orchestrator/api/*`),
so a DSH upgrade does not break it as long as the routes do.

The API is guarded by `sameOrigin()` in `src/app/http.ts`: the `Host` header must be
`127.0.0.1:<port>` or `localhost:<port>`, and every request carries
`x-orchestrator-request: 1`. A local process satisfies both; no token is involved.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/lanshi17/dsh-booth/master/install.sh | sh
```

That installs Rust, Node 22, pnpm and dsh when they are missing, clones this repository
under `~/.dsh/src/dsh-booth`, installs the `booth` binary, prepares the profile, and starts
the service. When the terminal can be used, it opens the console. `BOOTH_NO_UP=1` installs
only. `BOOTH_NO_CONSOLE=1` starts the service and stops there.

The same steps by hand:

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
booth wechat login                # connect an iLink bot from the terminal
booth wechat allow <微信用户 ID>  # allow another sender to contact the portal
booth provider                     # the model catalog the profile reports (no secret)
booth status                       # one shot: health, pause, setup, accounts
booth overview                     # problems, people waiting, executor load
booth portal                       # portal conversations and delegated tasks
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
booth send "把 README 里的错字改掉"  # message the portal agent
booth setup owner/name             # repository + allowed senders, from the shell
booth gh                           # the GitHub CLI the Source would use
```

For WeChat, run `booth wechat login`. It prints a fresh iLink link you can open on your phone,
waits for confirmation, and saves the bot credentials under the booth state directory with mode
`0600` without printing the token. The account that confirms the QR is added to the allow-list
automatically. The saved session is reused by later `booth wechat login` runs, so restarting booth
does not require another scan. Run `/setup owner/repo` in booth if the profile is not configured yet. Other
senders need `booth wechat allow <微信用户 ID>`. Check `booth sources --json` for poll and
delivery health.

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

Commands the one-shot CLI also has keep the same names: `/overview`, `/status`, `/portal`, `/tickets`,
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
| Portal and workers | The main Session talks to the user, plans local work and coordinates the `worker-<ticket>` Sessions |
| Several agents in parallel | `maxWorkers` (3 by default) runs Tickets side by side, plus one extra slot so a person's message never waits for a free worker |
| Durable conversations | The local conversation thread and its task associations survive console and service restarts; `/new` starts a separate conversation |
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
  Session handles the conversation and may delegate worker tickets;
- the agent's acknowledgements, replies and notices are appended to `booth-outbox.jsonl`, which
  the console shows as `◆ agent` lines;
- the GitHub and Feishu Sources keep working as before — booth adds a door, it does not replace
  them.

The portal is the team's lead. It answers questions and discusses requests directly, creates
worker tasks for concrete work, forwards relevant follow-ups and brings worker results back to
you. Independent tasks can run in parallel; a dependent task waits for its prerequisites.

```text
  ❯ 请检查安装流程和文档，并分别修复发现的问题
  ↑ sent to the portal  b4e50b13
  ◆ agent · 我会分别检查安装流程和文档，然后汇总结果。
  ❯ 文档只改安装部分
  ◆ agent · 收到，我会把范围限制在安装说明。
```

Messages in the console and `booth send` share a conversation, including after a restart.
Use `/new` in the console to start another conversation. Ask the portal about progress, or use
`/portal`, `/overview`, `/tickets`, `/ticket <id>` and `/agents` to inspect execution. Tell the portal
when to stop or redirect work; it coordinates the affected workers.

After 20 seconds without a judgement the console reports that it is still waiting; `/sources`,
`/status` and `booth logs` help identify a Source, service or model problem.

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
| The portal answers but starts no worker | Conversation and simple questions do not require a worker |
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
marked and an **add provider** row at the end that opens the vendor list. Choosing a row writes
`agent-default-model` into the profile layer and restarts the service.

`/login` follows Pi's shape. The first row is the DeepSeek account; the rest are the common
vendors, one preset each — DeepSeek, OpenAI, Anthropic Claude, Google Gemini, xAI Grok, Groq,
Mistral, OpenRouter, OpenCode Zen, Moonshot Kimi, Z.ai GLM, Qwen, SiliconFlow, Volcengine
Doubao, MiniMax, Together, Fireworks, Cerebras, the local Ollama and LM Studio endpoints, and a
custom row. A preset only seeds the form: its base URL, wire protocol (`openai-completions`,
`openai-responses` or `anthropic-messages`) and default model are the vendor's own starting
points and stay editable. Typing filters the list, so `/login`, `anth` and Enter is enough;
`/login gemini` opens that preset directly. Every API-key login writes:

- an `llm-pi-ai.providers.<id>` entry in the profile layer — `apiKeyEnv`, `baseURL`, `api` and
  the model — naming a credential ref, never the secret;
- the key itself as `refs.BOOTH_<ID>_KEY` in `~/.dsh/.credentials.yaml` (mode 0600), beside the
  keys DSH already stores there;
- `agent-default-model`, so the next start uses it.

The local presets open with `local` already in the key field: an Ollama or LM Studio server
ignores the `Authorization` header, but DSH still wants a credential ref to name.

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
booth send "fix the readme"            # portal agent
booth send --agent omp "fix the readme"
booth send --agent claude "fix the readme"
booth send --agent codex "fix the readme"
booth send --agent cursor-agent "fix the readme"
```

In the console, a line starting with `@omp`, `@claude`, `@codex`, or `@cursor-agent` runs that named tool directly; an unprefixed line goes through the portal. The direct form runs outside the portal task board.

## Providers

`/provider` opens one screen for every provider the team uses. The rows are grouped the way omp groups them: Language, Typesafe, Search, and Documents. Each row lists providers already configured, then the common presets (DeepSeek, Command Code, OpenAI, Groq, TypeSafe, Tavily, and the rest). A preset asks for its key. `a` opens the full list. The language row is the model the team runs with. Typesafe is the decision model. Search and documents remember their own provider and model in the same profile.

## Decision

The portal does not pick among its own options, and a sub-agent does not bring an open choice to the portal first. Both hand the choice to `booth decide`. The call is a [System One](https://commandcode.ai/docs/provider#decision-models-typesafe-jev) request: one `state`, and up to 100 `questions`. A question is `noul`, `choice`, or `score`. `from` is `subagent` or `portal` and stays with booth.

The default route is the `commandcode` provider and `typesafe/jev`. Save another one with `booth decide --provider <id> --model <model> --save`. The same flags without `--save` override that route for a single call. The provider is one already in the profile.

A choice or score at confidence 0.7 or more is the route, and so is a noul that is clearly yes or no. Below that, or on an error, a sub-agent's call escalates to the portal. The portal calls `booth decide` again with `"from": "portal"`. If that call also cannot route, it escalates to the user. `booth up` installs this order into the team's prompt.

## License

GPL-3.0-only. `dsh-superagent`, which this console talks to, is MIT and remains a research
prototype; this project is not an official DeepSeek product.
