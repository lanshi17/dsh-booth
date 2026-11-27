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
- `/providers` reads the dsh `llm-pi-ai` provider directory instead of the current
  model set, so provider setup works before the first session; OMP's `models.yml` +
  SQLite credential storage is documented as the mapping.
- `/providers` Enter opens an in-TUI provider page (`name`, `displayName`, `api`,
  `baseURL`, `models`, `apiKey`) with Ctrl+S save, Ctrl+F discovery, Ctrl+X remove
  and a final `＋ new provider` row; no external editor or `/providers edit`.
- Unconfigured catalog rows are listed and can be configured in place; blank
  `apiKey` keeps the current key and `apiKey=!clear` removes it. Model-list edits
  retain existing per-model metadata.
- `/providers` retries the control directory during startup instead of silently
  showing model-derived rows with a misleading edit hint.
- Clipboard copy now falls back to `wl-copy`/`xclip`/`pbcopy` locally, while
  Ctrl+V validates image magic so xclip text no longer attaches as an image.
- tmux now receives raw OSC 52 plus `tmux load-buffer`; the DCS passthrough
  envelope is conditional on `allow-passthrough` (3.3+ drops it when off), and
  SSH+tmux Ctrl+V reads `tmux show-buffer`.
- Provider edits skip unchanged `api`/`models`/`baseURL` fields, preserving rich
  model entries such as per-model context windows and reasoning capabilities.
- The external provider editor requests a full repaint and a synthetic frame after
  exiting `$EDITOR`, removing the gray alternate-screen state.
- `/clear` closes the live session so the next prompt starts a genuinely fresh
  dsh context, with a context-specific notice; `/btw` history is session-scoped,
  capped, and user-only readable.
- `/plugins` lists live plugin/bundle state from dsh's `pluginManager`, and
  `enable`/`disable` apply through it; `/todo expand` shows completed rows and
  resets per session; `/skills` results are project-first and capped.
- `/stats` validates flag values and `/session` rejects unexpected subcommand
  arguments; `/jobs kill` reports the backend outcome.

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
