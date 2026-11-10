# Dependency closure (P0) + Headless trim (P1)

## Method

Static BFS over internal path / `workspace = true` dependencies.
Path specs are resolved **relative to the owning crate** (workspace
dependency paths remain workspace-root relative).

Keep-set is the fixed point of `dependencies` + `build-dependencies` +
`dev-dependencies` starting from `codex-exec`.

## Counts

| Set | Count |
|---|---|
| Original workspace members | 166 |
| Headless workspace members | 138 |
| Removed from workspace | 32 |
| core-required (prod) | 98 |
| headless-required (prod) | 130 |

## Excluded (high level)

`cli`, `tui`, cloud-tasks*, voice-host, realtime-webrtc, v8-poc,
code-mode-runtime/host, and other non-headless product crates.
See `removable-paths.txt`.

## Verification

`cargo check -p codex-exec` — pass (V8 patch removed from `[patch.crates-io]`).

## P1 follow-ups (2026-11-08)

- `default-members = ["exec"]`
- `just exec` / alias `c` → `cargo run -p codex-exec`
- `verify_tui_core_boundary.py` skips when `tui` is not a workspace member
- Full `cargo build -p codex-exec` needs `libssl-dev` (link); `cargo check` already passes
- P2 `codex-external-agent` deferred: see `docs/archive/plans/2026-11-01-codex-core-minimal-fork.md`
  and `superagent-forge/docs/ROADMAP.md` ("不继续 Codex Fork 的 P0/P1…不实现外部 Runtime 对 Codex 的集成")
