# Workflow Strategy

This fork’s merge gate is intentionally narrow: validate the independent Rust
workspaces that this repository actively maintains. Upstream Codex Bazel,
`rust-ci`, SDK, and postmerge suites remain in-tree for reference / manual
dispatch, but they are **not** required to merge.

## Merge gate (`blocking-ci`)

| Job | Purpose |
| --- | --- |
| `forge-ci` | `btae-eval` + `superagent-forge`: fmt, Clippy `-D warnings`, all-target tests, clean worktree |
| `codespell` | Spelling |
| `cargo-deny` | Dependency policy |
| `blob-size-policy` | Oversized blob guard |
| `CI required` | Fan-in: every dependency must be `success` |

`forge-ci.yml` runs on Ubuntu 24.04 with Rust 1.95.0. Bubblewrap is installed for
sandbox tests. Model credentials and live model calls are not used.

Manual diagnosis: `workflow_dispatch` on `forge-ci` or `blocking-ci`’s children.

## Opt-in only (not merge-blocking)

| Workflow | How to run |
| --- | --- |
| `postmerge-ci` | `workflow_dispatch` (calls `rust-ci-full` + `v8-canary`) |
| `bazel.yml` / `rust-ci.yml` / `sdk.yml` / `repo-checks.yml` | `workflow_call` / `workflow_dispatch` where available |
| Tag releases (`rust-release`, `python-sdk-release`, `rusty-v8-release`) | Push matching tags |

## Hosted runners

CI uses GitHub-hosted runners (`ubuntu-24.04`, `windows-latest`, `macos-15`, …).
No private `codex-forge-runners` group is required for the Forge merge gate.
