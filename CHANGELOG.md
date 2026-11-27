# Changelog

## [0.5.1] — 2026-11-27

### Added

- **Per-command OMP flows**: `OMP_COMMANDS.md` records the user experience of all
  89 OMP builtin commands and how booth reproduces each one.
- `/copy code|cmd|link|local` copies the last fenced block, shell command, link
  or the local state directory.
- `/todo append|start|done|drop|rm|expand|collapse` mutates the list and the HUD,
  committing through the control plane.
- `/jobs full`, `/jobs kill all`, `/session delete`, `/usage reset`, `/usage show`
  and the expanded `/mcp test|enable|disable|reload` flows.
- `/skills search|installed` and `/collab start|view|list|stop|status`.
- `/compact soft|remote|handoff` mapping, `/goal budget` capability notice,
  `/git <revision>` patches, `/stats --port|--host` validation, and provider-aware
  `/login`/`/logout`; `/btw` keeps a browsable history file.

### Fixed

- `/switch [model]` is the OMP session-only model command again; saved presets
  apply through `/modelpreset switch`.
- `/open` validates its optional link argument, and login/logout forward provider
  arguments.
- Disabled MCP rows remain listed while excluded from the next session.

## [0.5.0] — 2026-11-26

### Added

- **OMP client parity**: slash-command menu, aliases, argument completion, prompt templates,
  sessions, skills, providers, models, settings, guided goals, diagnostics, side agents and
  loop runs.
- **Screen recording**: `/record` writes JSONL frames under `$BOOTH_STATE_DIR/recordings`, and
  `booth play <file>` replays them in the alternate screen.
- **HTML export**: `/export [--themes] [path]` writes a self-contained HTML transcript; `.md`
  paths keep the Markdown export.
- **Todo round-trip**: `/todo edit`, `/todo import [path]` and `/todo export [path]` round-trip the
  session todo list through Markdown; commits use the control plane's `todos.commit`.
- **Finished subagents**: `/agents` and `/hub` include finished subagent sessions from the
  persisted session log.

### Fixed

- Copy and paste use OSC 52 and terminal paste events, including SSH and tmux sessions.
- Session restart preserves the active session through the documented restart marker.
- `booth down` and `booth restart` stop the supervisor itself; the pid file no longer points at the
  kernel child. Repeated profile installs keep one managed `booth-control` insert, and daemon tests
  isolate `DSH_HOME` from the operator profile.

## [0.4.1] — 2026-11-19

### Fixed

- The console binds the exact launch directory, so a stale root cannot receive work.
- Copying reaches the operator's clipboard over SSH and tmux through OSC 52.

### Changed

- The terminal owns mouse selection, and copy mode handles text the terminal cannot select.

## [0.4.0] — 2026-11-19

### Added

- Transcript copy and paste, CSS Agent support and the subagent board.
- Component supervisor, component inspection and dsh-superagent registration.

## [0.3.0] — 2026-11-11

### Added

- Supervised component lifecycle and live subagent snapshots.

## [0.2.0] — 2026-11-06

### Added

- Initial ACP console with transcript rendering and session resume.

## [0.1.0] — 2026-11-05

### Added

- First booth release.
