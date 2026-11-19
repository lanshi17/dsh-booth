# Changelog

## [0.4.0] — 2026-11-19

### New features

- **Transcript copy and paste**: dragging over the transcript selects output and puts it on the
  system clipboard on release (`Ctrl+Shift+C` or `Ctrl+Insert` copies again); the alert row
  reports the result. `Ctrl+V`/`Shift+Insert` read the clipboard through
  `wl-paste`/`xclip`/`xsel`/`pbpaste` when the terminal does not send a bracketed-paste event.
  Wide CJK characters and wrapped rows copy as they appear on screen.

### Improvements

- **The working row times only real work**: the bottom-left spinner now appears for a foreground
  command or a subagent whose stage is `working`. A portal judgement still pending and a ticket
  waiting for a person no longer leave a counter running; the header pulse and transcript still
  report those states.

[0.4.0]: https://github.com/lanshi17/dsh-booth/releases/tag/v0.4.0

## [0.3.74] — 2026-11-19

### Improvements

- **Scrollable transcript**: the mouse wheel scrolls the console three rows at a time, alongside
  `PgUp`/`PgDn`; an open modal keeps the wheel. The alert line reports a scrolled view and how to
  return to the newest output.
- **Arrows stay with history**: `↑`/`↓` browse recalled commands and never move the transcript.

[0.3.74]: https://github.com/lanshi17/dsh-booth/compare/v0.3.73...v0.3.74

## [0.3.73] — 2026-11-19

### New features

- **CSS Agent support**: `booth css` and `/css` read CSS Agent's loopback `/v1/health` and
  `/v1/subagent/manifest`, and an optional `SubagentProtocol.snapshot()` feed
  (`BOOTH_CSS_DIR`, `BOOTH_CSS_FEED`). Protocol, roles, report states, activation limits and
  live CSS tickets are shown; an outage never hides the team's own work.
- **Subagent board**: `booth subagents` and `/subagents` list every working and waiting worker
  Session with its stage, latest report summary, worker Session and update time. Pages follow
  the service's cursor instead of the first 50 rows; CSS Agent's `subagent/1` tickets appear
  beside them when a snapshot feed is configured.

### Improvements

- **Specific interaction feedback**: the working row names the action (`loading /subagents`,
  `restarting the service`, `portal · 2 waiting for a judgement`) and, when idle, the worker
  that is producing output. The team pulse distinguishes `running` from `active`.
- **Progress between stages**: a followed ticket prints each new worker report summary once,
  so work between `working` and `closed` is no longer silent.

### Migration notes

- CSS Agent reads are optional and loopback-only; without `BOOTH_CSS_URL` booth probes the
  documented default `http://127.0.0.1:8765` and reports it down until a service answers.

[0.3.73]: https://github.com/lanshi17/dsh-booth/compare/v0.3.72...v0.3.73

## [0.3.72] — 2026-11-19

### New features

- **Component supervisor**: `booth` now owns a small authenticated loopback supervisor. Components register their ID, process, management URL, and version, renew a lease, and disappear automatically after a crash.
- **Component inspection**: `booth components` and `booth components --json` show the components currently registered with the supervisor.
- **dsh-superagent registration**: the bundled agent registers after its runtime is ready, renews its lease while running, and unregisters during graceful shutdown. `booth restart` replaces the agent component while keeping the supervisor alive.

### Improvements

- **Reliable conversation history**: legacy inbox and outbox records are imported idempotently into the local conversation archive, with locking, partial-record recovery, bounded transcript rendering, and background session recovery.
- **Faster startup checks**: independent GitHub and Feishu setup checks run concurrently where safe, while preserving prerequisite error ordering and cancellation.
- **More responsive console**: local replies are polled independently from slow management API calls; decisions and model catalog requests no longer wait behind ticket pages.
- **Safer lifecycle operations**: startup, restart, readiness probes, PID reuse checks, and cross-process lifecycle locking now fail clearly without targeting unrelated processes.
- **Older DSH compatibility**: router delegation tools are filtered to the tools available on the host, so persisted sessions can recover on older DSH installations.

### Migration notes

- Source builds apply the bundled superagent integration patches during `booth init`.
- The supervisor stores its private token and component registry under `BOOTH_STATE_DIR`. Existing booth state is retained and legacy conversation files remain in place after import.

[0.3.72]: https://github.com/lanshi17/dsh-booth/compare/v0.3.71...v0.3.72
[0.3.71]: https://github.com/lanshi17/dsh-booth/releases/tag/v0.3.71
