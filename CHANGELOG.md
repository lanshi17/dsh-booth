# Changelog

Published release history is limited to the release lines still tagged and published:
`v0.4.0`, `v0.3.0`, `v0.2.0` and `v0.1.0`.

## [Unreleased]

### Fixes

- **The console starts from the directory it was launched in**: opening `booth` binds the
  ticket-directory root to that exact working directory, so a root left over from an earlier
  launch elsewhere no longer receives the work. No Git checkout is required. The binding is shown
  in the transcript and agent instructions wait for it; an explicit
  `/setup owner/name <directory>` still chooses a different root until the next console launch.

### Changes

- **Copy and paste belong to the terminal**: the mouse is no longer captured and the in-app
  transcript selection, clipboard helper commands and `Ctrl+V` fallback are gone. The terminal's
  own selection, copy shortcuts, scrolling and bracketed paste work directly, including a drag
  that crosses the window. `PgUp`/`PgDn` remain for scrolling the transcript itself.

## [0.4.0] — 2026-11-19

Everything since `v0.3.0`; the `0.3.x` patch tags were retired.

### New features

- **Transcript copy and paste**: dragging over the transcript selects output and puts it on the
  system clipboard on release (`Ctrl+Shift+C` or `Ctrl+Insert` copies again); the alert row
  reports the result. `Ctrl+V`/`Shift+Insert` read the clipboard through
  `wl-paste`/`xclip`/`xsel`/`pbpaste` when the terminal does not send a bracketed-paste event.
  Wide CJK characters and wrapped rows copy as they appear on screen.
- **CSS Agent support**: `booth css` and `/css` read CSS Agent's loopback `/v1/health` and
  `/v1/subagent/manifest`, and an optional `SubagentProtocol.snapshot()` feed
  (`BOOTH_CSS_DIR`, `BOOTH_CSS_FEED`). Protocol, roles, report states, activation limits and
  live CSS tickets are shown; an outage never hides the team's own work.
- **Subagent board**: `booth subagents` and `/subagents` list every working and waiting worker
  Session with its stage, latest report summary, worker Session and update time. Pages follow
  the service's cursor instead of the first 50 rows; CSS Agent's `subagent/1` tickets appear
  beside them when a snapshot feed is configured.
- **Component supervisor**: `booth` owns a small authenticated loopback supervisor. Components
  register their ID, process, management URL, and version, renew a lease, and disappear
  automatically after a crash.
- **Component inspection**: `booth components` and `booth components --json` show the components
  currently registered with the supervisor.
- **dsh-superagent registration**: the bundled agent registers after its runtime is ready,
  renews its lease while running, and unregisters during graceful shutdown. `booth restart`
  replaces the agent component while keeping the supervisor alive.

### Improvements

- **The working row times only real work**: the bottom-left spinner appears for a foreground
  command or a subagent whose stage is `working`. A portal judgement still pending and a ticket
  waiting for a person no longer leave a counter running; the header pulse and transcript still
  report those states.
- **Scrollable transcript**: the mouse wheel scrolls the console three rows at a time, alongside
  `PgUp`/`PgDn`; an open modal keeps the wheel. The alert line reports a scrolled view and how to
  return to the newest output. `↑`/`↓` stay with command history.
- **Specific interaction feedback**: the working row names the action (`loading /subagents`,
  `restarting the service`) and, when idle, the worker that is producing output. The team pulse
  distinguishes `running` from `active`.
- **Progress between stages**: a followed ticket prints each new worker report summary once,
  so work between `working` and `closed` is no longer silent.
- **Reliable conversation history**: legacy inbox and outbox records are imported idempotently
  into the local conversation archive, with locking, partial-record recovery, bounded transcript
  rendering, and background session recovery.
- **Faster startup checks**: independent GitHub and Feishu setup checks run concurrently where
  safe, while preserving prerequisite error ordering and cancellation.
- **More responsive console**: local replies are polled independently from slow management API
  calls; decisions and model catalog requests no longer wait behind ticket pages.
- **Safer lifecycle operations**: startup, restart, readiness probes, PID reuse checks, and
  cross-process lifecycle locking now fail clearly without targeting unrelated processes.
- **Older DSH compatibility**: router delegation tools are filtered to the tools available on
  the host, so persisted sessions can recover on older DSH installations.

### Migration notes

- CSS Agent reads are optional and loopback-only; without `BOOTH_CSS_URL` booth probes the
  documented default `http://127.0.0.1:8765` and reports it down until a service answers.
- Source builds apply the bundled superagent integration patches during `booth init`.
- The supervisor stores its private token and component registry under `BOOTH_STATE_DIR`.
  Existing booth state is retained and legacy conversation files remain in place after import.

## [0.3.0] — 2026-11-11

### New features

- **Choose the worker per instruction**: dsh is the default. `booth send --agent` or an
  instruction beginning with `@omp`, `@claude`, `@codex` or `@cursor-agent` hands the line to
  that coding agent.
- **Decider routing**: the portal and subagents offer choices to `booth decide`, backed by
  CommandCode's `typesafe/jev` (`/systemone`). `--provider` and `--model` override the route.
- **Provider screen**: `/provider` configures language models, typesafe, web search and document
  handling on one screen; common providers are preset, and selecting one opens its key form.
- **Login list**: `/login` filters by name (`/login gemini` opens that form); local Ollama and
  LM Studio presets start with a placeholder key.
- **Installer**: one `curl … | sh` command installs dependencies, prepares the profile and
  starts the service.
- **Executor sizing**: concurrency follows the machine's core count rather than a fixed three;
  subagents use CommandCode's `deepseek/deepseek-v4.1-flash`.

## [0.2.0] — 2026-11-06

### New features

- **One provider configuration**: `/provider`, the header and `booth provider` read the service's
  `GET /models` catalog, the same one the web panel shows; booth no longer writes
  `provider.json` or passes a generated `provider.yml` to `--patch`.
- **Model selection**: `/model` writes `agent-default-model` into
  `~/.dsh/profiles/<name>/cordis.patch.yml`, the layer DSH already composes.
- **Credential references**: the API-key branch of `/login` writes
  `llm-pi-ai.providers.<id>` and stores the secret in `~/.dsh/.credentials.yaml` under
  `refs.BOOTH_<ID>_KEY`.
- **OAuth default**: a successful account login removes booth's `agent-default-model`.
- **Lifecycle cleanup**: `booth up` no longer passes `--patch` or injects `BOOTH_KEY_*`.

### Migration notes

- The default model now lives in the DSH profile. An existing `provider.json` is migrated once
  and renamed `provider.json.migrated`; it is no longer read.

## [0.1.0] — 2026-11-05

### New features

- **First release**: bare text (or `/send`) hands an instruction to dsh-superagent, and its
  judgement, ticket progress and replies come back on the same screen.
- **Conversation continuity**: follow-up instructions continue the same ticket across restarts;
  `/new` starts another. `/setup owner/repo` configures the repository, allow list and console
  Source once.
- **Login and evaluation**: `/login` supports DeepSeek account PKCE and API keys; `/eval
  off|observe|auto` decides how much work proceeds without a person.
- **Work inspection**: `/agents` shows router/worker orchestration, parallelism and memory
  locations.
- **Console editing**: fuzzy command palette, readline editing with a kill ring, restart-safe
  `Ctrl+R` history, and a braille spinner working row.

[0.4.0]: https://github.com/lanshi17/dsh-booth/releases/tag/v0.4.0
[0.3.0]: https://github.com/lanshi17/dsh-booth/releases/tag/v0.3.0
[0.2.0]: https://github.com/lanshi17/dsh-booth/releases/tag/v0.2.0
[0.1.0]: https://github.com/lanshi17/dsh-booth/releases/tag/v0.1.0
