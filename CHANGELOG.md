# Changelog

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
