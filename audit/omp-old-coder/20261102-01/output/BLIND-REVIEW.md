# Blind review — old-coder candidate (phase 1, round 1)

## Source state and freshness

- Subject: `workspace/btae-eval`, 55 files, tree SHA256 `46d6c1801a41010f5bf5af249b0477c4dfdbd1fe6042e05c5c5fb52c6720c0c1`.
- Verified before and after all probes/mutants with `python3 gauntlet.py --verify-only`; both matched `source-state.json`. No candidate file was modified; the frozen manifest is unchanged.
- The snapshot is an untracked working-source copy; Git HEAD does not describe these files (stated by `source-state.json`; not independently cross-checked against Git, which is out of the four-input scope).
- Fresh context: only the four task inputs plus the three mandated instruction files were used before results. No prior EVIDENCE, acceptance report, builder conversation, hidden evaluator data, credentials, or outside repository were read. SPEC is retrospective/autonomous: `spec approval: not obtained`, so correlation is broken for task context only, not for model.
- Environment: rustc/cargo `1.95.0`, Python `3.14.4`; all Cargo work `--offline`, no installs. Candidate build target `audit/target`; harness `audit/harness`; mutants `audit/mutants/m{1,2,3}` (disposable copies).

## 1. The run (blind)

Command: `python3 gauntlet.py output/blind-baseline`

| Layer | Command (as executed by gauntlet.py) | Exit |
|---|---|---|
| tests | `cargo test --all-targets --locked --offline -j 2 -- --test-threads=4` | 0 |
| types | `cargo check --all-targets --locked --offline -j 2` | 0 |
| lint | `cargo clippy --all-targets --locked --offline -- -D warnings` | 0 |
| format | `cargo fmt --all --check` | 0 |

Result: `BASELINE_LAYERS_COMPLETE; NOT A FULL REVIEW VERDICT`. Test totals from `output/blind-baseline/tests.stdout.txt`: 54 passed, 0 failed (`1 + 0 + 0 + 4 + 23 + 11 + 15` across binaries; `dsh_adapter` 23). Output directory was fresh (`mkdir(exist_ok=False)`); no stale artifact was read. The entry point refuses on manifest mismatch (negative control below), on any nonzero layer (explicit `sys.exit`), on a wrong layer manifest (final `assert`), and re-verifies the tree after the layers.

## 2. Spec against contract

The SPEC's S1–S6/M1–M4 are consistent with the task contract (offline read-only parser, public Rust API, no product dependency on DSH/OMP, review only). No scenario authorizes writing; M1–M4 are reviewer constraints, not product behavior. No approved exclusion was found to be described inaccurately. Two spec-level ambiguities worth naming, not scored as defects:

- S3 says "exit 0 + final + valid session + all turns normally ended is necessary"; a turn with zero steps and `reason.kind=completed` satisfies it. The adapter then returns `completed=true`, `usage_status=Unknown` (probe P10). This matches the stated rule; whether a zero-step turn should count as CLI completion is a spec question, not a code contradiction.
- S6 asks to examine "misleading provenance claims". `text_sha256` is SHA256 over concatenated text payloads with no event-boundary encoding; `"ab"+"c"` and `"a"+"bc"` both hash to `ba7816bf…` (P19). The field comment and DSH-ADAPTER say "concatenated", so the claim is literally accurate; it is listed as a limit (not a finding) because no summary field claims the digest identifies a transcript.

## 3. The tests / test-gaming

Read `tests/dsh_adapter.rs` (23 tests) and the module. Tests feed real sanitized fixtures and synthetic transcripts; no mock of the unit; no implementation keyed to fixture values was found. I chose three independent mutants (not the builder's list; no prior list was available) and proved divergence on concrete input before counting survival.

Mutant procedure: copy `workspace/btae-eval` to `mutants/mN` (excluding `target`), apply exactly one unique string replacement, add `examples/probe.rs`, run `cargo test --offline --all-targets -- --test-threads=4` with `CARGO_TARGET_DIR=mutants/target-mN`, then run the probe. Original-side results came from `harness` (path dependency on the frozen workspace) using the identical probe source.

| Mutant | Mutation (src/dsh.rs) | Divergence input | Original | Mutant | Suite |
|---|---|---|---|---|---|
| m1 | `if reasoning > output` → `>=` (l.958) | `{outputTokens:5,reasoningTokens:5,totalTokens:10}` | `OK completed=true status=Complete` | `ERR reasoningTokens exceeds outputTokens at event 4` | 54 passed → **SURVIVED** |
| m2 | `if total > max_total_bytes` → `>=` (l.434) | session line exactly 35 bytes, limit 35 | `OK bytes=35` | `ERR exceeds the 35-byte total limit` | 54 passed → **SURVIVED** |
| m3 | delete `turn_end while a step is open` guard (l.885-887) | session, turn_start, step_start, turn_end (no step_end) | `ERR protocol: turn_end while a step is open` | `OK completed=false steps=0/1` | 54 passed → **SURVIVED** |

`mutants/results.json` records return codes and captured tails. All three mutants also survive the 23-test adapter file alone. This is a coverage/assurance finding, not a candidate behavioral defect: the frozen code is correct on each divergence input. The suite's existing limit tests only exercise over-limit (`6 > 5`), never equality; the reasoning test exercises `reasoning > output`, never equality; no test contains `turn_end` with an open step.

Attacks that produced nothing (recorded as required): implementation keyed to fixture constants; assertions satisfied by unused fields; mocks swallowing logic; duplicate-key detection only at top level (nested duplicate was correctly rejected, P4); unknown-event fallthrough; overflow of per-field sums; final-before-turn-end and event-after-final. No test-gaming was observed beyond the three unpinned boundaries above.

## 4. Checkers

`gauntlet.py` is the only home-grown gate. Negative control: `python3 -c 'import gauntlet; gauntlet.verify({})'` → `RuntimeError: frozen source manifest mismatch` (fails closed; observed). Reading the code: no `|| true`, no `2>/dev/null`; each layer's non-zero status exits immediately; the expected-layer manifest is asserted after the loop; a missing command raises and exits non-zero. Scope limit: the gate checks the frozen tree plus tests/types/lint/format only — it has no coverage, mutation, property, or random-order layer, so it cannot substantiate any such claim (and the SPEC does not ask it to). The checker's `results.json` is written after each layer, so a crash mid-run still leaves partial evidence, but the shell entry point exits non-zero, which is the contract that matters.

## 5. Mapping, both directions

| Row | Evidence in this blind pass | Status |
|---|---|---|
| S1 | P1: 5/20/3/7/4 → input 28, output 7, total 35, status Complete; fixture totals reproduced (P17): dev 1,871,486 / 45,968 / 1,917,454 over 23/23 steps; smoke 34,746 / 10 / 34,756 over 1/1. | PASS (probed) |
| S2 | P10 zero steps → Unknown; P17 unfinished/coverage fields consistent; existing tests cover missing usage and unfinished step, but I did not independently re-construct the two-step missing-usage case. | PARTIAL (test-mapped, not fully re-probed) |
| S3 | P12 final early and event after final → protocol errors; P11 empty → Ok completed=false; P20 malformed reason object → InvalidJson; nonzero-exit/cancelled paths only via existing tests. | PARTIAL |
| S4 | P4 nested duplicate key; P5 unknown type/status/usage field; P6 alias conflict; P7 negative/float/bool/string/2^64/1e400; P8 sum overflow; P9 total<output, full-bucket mismatch, reasoning>output; P3 depth 127/200/2000 all `InvalidJson: JSON nesting limit exceeded`, no crash. Poison stickiness observed only through the existing test (adapter suite green). | PASS for probed classes; stickiness test-mapped |
| S5 | P2 whole vs bytewise chunks identical; P13 line at 64 accepted, 65 rejected, CRLF at 64 accepted, CR/LF split across chunks accepted; exact total 35 accepted, 34 rejected; max_events boundary source-checked; P15 CR-only gives protocol error; P3 recursion bounded. | PASS |
| S6 | `dsh.rs` has no `std::process`, `std::env`, `Command`, `File`, `fs`, or network symbols (grep empty); only `crate::digest` imported; `pub mod dsh` exports; no `Observation` constructed; Cargo.toml/lock frozen in the manifest (no dependency diff). | PASS (static + probes) |
| M1 | Candidate manifest unchanged before/after; failures/mutants preserved under `output/`, `harness/`, `mutants/`. | PASS |
| M2 | Exact paths, versions, commands, exit codes and test counts recorded; baseline fresh run. | PASS |
| M3 | Coverage, fuzz/proptest, random order, cargo-mutants, version matrix, canary, human grading all recorded as UNAVAILABLE/not run; deterministic generation is labeled SUBSTITUTED, never a pass. | PASS |
| M4 | No dependencies installed, no assertions weakened, no acceptance criteria edited; review only. | PASS |

Reverse direction: every one of the 23 adapter tests maps to S1–S5; no test lacks a scenario. Demonstrated failure modes without a row: none found; opaque-event semantics are explicitly out of scope in DSH-ADAPTER ("只有计数").

## 6. Findings (proposals; human grades)

1. **F1 — `src/dsh.rs:958-962`, missing boundary test for `reasoningTokens == outputTokens`.** Valid input rejected by mutant m1, which survives all 54 tests. Class: missing-assurance. Severity proposal: low.
2. **F2 — `src/dsh.rs:434-441`, missing exact-limit test for total bytes.** A transcript of exactly `max_total_bytes` is accepted by the candidate but rejected by mutant m2, which survives all 54 tests. Class: missing-assurance. Severity proposal: low.
3. **F3 — `src/dsh.rs:885-887`, missing test for `turn_end` with an open step.** Mutant m3 changes the caller-visible contract from `Err(Protocol)` to `Ok(incomplete)` and survives all 54 tests. Class: missing-assurance. Severity proposal: low-medium.
4. **F4 — `src/dsh.rs:37-41`, description overstates fail-closed coverage.** Comment says unknown event fields are rejected; opaque `thinking`/`tool_call`/`tool_result`/`trace`/`tool` payload fields are accepted unvalidated (probe P3). `protocol/DSH-ADAPTER.md` is accurate ("opaque: only count"), so this is a description defect, not a behavioral one. Class: description. Severity proposal: low.

No behavioral defect was found in the frozen candidate on the attacked surfaces. Findings F1–F3 are test-suite assurance gaps demonstrated by non-equivalent mutants; F4 is a doc-comment mismatch. Details, triggers, commands and impact are in `output/findings.json`.

## 7. Honest limits of this phase

- Coverage percentage: **UNAVAILABLE** (llvm-cov/llvm-tools absent; not installed).
- Fuzzing/property-based tooling: **UNAVAILABLE**; deterministic chunking/boundary probes are **SUBSTITUTED**, not a property-layer pass.
- Random-order suite health: **UNAVAILABLE** on stable Rust; order dependence untested.
- Tool-based mutation: **UNAVAILABLE**; three manual mutants, each divergence proven, all survived.
- Version matrix: **UNAVAILABLE** (only rustc 1.95.0; declared `rust-version = 1.85` not exercised).
- Real DSH CLI not run; only two sanitized fixtures. Other DSH versions, live streaming, cancellation/timeout (runner-owned), and opaque payload semantics are untested.
- Dependency audit: N/A (no dependency change); capability review is static grep only.
- Human grading not obtained; no canary was requested or run.
- These are blind-phase results only. Prior EVIDENCE is intentionally not compared here; the blind record is now append-only.
