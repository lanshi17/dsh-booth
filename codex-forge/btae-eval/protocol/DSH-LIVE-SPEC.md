# DSH live process collector — engineering smoke v1

Status: in-progress. Spec approval: not obtained (autonomous run).
Isolation: branch feat/dsh-live-collector in /tmp/codex-forge-dsh-live-20261102; landing checkout stays unchanged until verification. Pure Rust 2024 orchestrator; external DSH deployment may use Node. No new Cargo dependencies or installed tools. Uses existing cargo/Clippy/bwrap/nix and stdlib test helpers. Git: normal commits and fast-forward landing, no history rewriting. Files: src/dsh_live.rs, src/main.rs, src/lib.rs, tests/dsh_live.rs, test fixture binary, Cargo binary registration, protocol/status/report docs and reproducible verification script.

## Executable scenarios

S1 A self-contained executable or explicitly selected runtime + code-only bundle receives only fixed `--profile headless --json -` argv and a bounded prompt file on stdin. No shell. Default network namespace disabled. Host HOME, project, evaluator and environment unavailable. Test a fake DSH with real process IO and hostile probes; deploy actual DSH for engineering smoke separately.
S2 Only explicit credential environment names ending _API_KEY/_AUTH_TOKEN are passed via process environment, never argv, serialized records or raw output. Dedicated public profile directory copied into private ephemeral DSH_HOME; reject symlink/special files and credentials/.env profile inputs. Runtime/bundle are trusted deployment artifacts, read-only. No host DSH_HOME or sessions mounted.
S3 stdout is incrementally fed to existing DshAdapter, stderr independently drained; channel/input/total stream/line/event/prompt/wall limits bounded. Raw streams are not retained; record byte counts and SHA256. Valid exit0 + completed summary yields completed. Missing final, nonzero, malformed streams cannot become empirical success; hidden result and empirical task success are null.
S4 Host cancel sentinel, wall deadline, stream cap and parser error kill the process group and PID namespace, including descendants after direct child exit. Cancel/time/error outcomes dominate any late final. Reader threads are drained/joined or explicitly marked incomplete; no retry and no billing/hard token-cap claim.
S5 Existing output directories fail; create private output (0700), started record before spawn, final record binds prompt/runtime/entrypoint/profile/workspace/limits/network and sanitized summary. Output location must not overlap input/bundle/profile. State cleanup removes ephemeral profile/session writes. Snapshot hashes are trusted-host integrity only, not a signature or independent Review.
S6 Shared host networking requires explicit --allow-network. It is engineering-only with no endpoint allowlist, not production egress isolation. Actual single-turn ark/deepseek smoke may use this authorized mode; no five-group or superiority claim, no readiness changes. If deployment/model fails, keep honest failure result and distinguish from synthetic success.

## Failure model and checks

- Sensitive host input/execution leakage: bwrap isolated mounts, cleared env, fixture reads/probes + credential redaction test.
- Endless output/deadlock or descendant pipes: bounded chunks/channel, concurrent drain, flood/timeout/cancel/descendant tests.
- False completion/approval: malformed/missing final/nonzero/late-final tests, Observation deserialize rejection.
- Overwritten/tampered evidence: create_new records and output collision/hash checks.
- Unsafe paths/profiles/runtime: reject unsafe relative entrypoint, symlinks and directory overlaps before spawn.

Full suite, Clippy -D warnings, fmt and readiness(expected1) after changes. Observe behavioral RED via a nonexecuting stub before implementing process logic. Add 3 targeted manual non-equivalent mutants; no compile-failure kills. Coverage threshold/fuzz/proptest/random-order/min-Rust matrix unavailable/not run; do not claim full Tier3 assurance. Independent verification for this new source state: not performed unless separately executed; prior task's two reviews are history only. Record executable/profile/model-version limits and actual smoke result in EVIDENCE. Shared-network fixture is not formal experimental data.

## Scope revision — implemented collector boundary (autonomous, no human SPEC approval)

The initial S1/S2/S4/S5/S6 above describe the desired integrated launcher and remain historical intent. This delivery is the lower-level `run_live` / `collect` supervisor. OS filesystem/network/PID namespaces, dedicated profile provisioning, bundle/entrypoint/workspace digesting, source/output overlap checks and profile cleanup are caller responsibilities; there is no built-in `--allow-network` or endpoint allowlist. Only the launcher executable, argv, prompt, parser/process limits, observed streams and sanitized summary are bound by this collector. CLI `--pass-env NAME` permits explicit named environment entries, without the earlier suffix restriction; values are never emitted by the collector. Polling uses nonblocking pipes, not reader threads. Real smoke supplies bwrap separately and shared networking is explicitly recorded in its deployment report. This is not a production-isolated task runner.

The setup additionally enables the existing nix dependency's `fs` feature for safe `fcntl(O_NONBLOCK)`. No dependency version or lockfile changed. Initial tests did not start from a complete behavioral stub; one concrete RED for exit-zero/missing-final was observed, followed by regression tests and manual non-equivalent fault controls. This is a declared process limitation, not a claim of full TDD/Tier 3 completion.
