# ROUND 2 BLIND REVIEW — DSH adapter review-gap hardening

Blind: no prior report, EVIDENCE, session, or other audit output was read. Only the four named inputs were opened; `TASK-CONTRACT.md` was **not present** in the audit root (verified by `find`), so contract-vs-spec could not be attacked. Observed source state: `eee74ab153cc739b0855e1613b19bc9c846ed0f83700d66abbc7cfb610554f70` (tree = sha256 of sorted compact `files_sha256` manifest; recomputed and consistent), 53 files.

## Commands and actual results

| Layer | Command | Actual |
|---|---|---|
| baseline | `python3 verify.py output/reviewer-baseline` | all four layers matched expected exits; final line `BOUNDED_REVIEW_GAP_VALIDATION_PASSED` |
| tests | `cargo test --all-targets --locked --offline -j 2 -- --test-threads=4` | exit 0; **57 passed, 0 failed** (1+4+26+11+15; three 0-test binaries) |
| lint | `cargo clippy --all-targets --locked --offline -- -D warnings` | exit 0 |
| format | `cargo fmt --all --check` | exit 0 |
| readiness | `cargo run ... check-ready datasets/pilot.json` | exit 1 (expected) |
| mutation | `python3 mutations.py ...` | 3/3 KILLED, each exit 101, assertion panic, no compile error |
| independent mutant rerun | fresh copies + **isolated** `CARGO_TARGET_DIR` per mutant | 3/3 same named test FAILED, same panic lines; no stale-target artifact |

Named mutation attribution (behavioral, not compile/infrastructure):
- `reasoning >`→`>=` (src/dsh.rs:960): `reasoning_equal_to_output...` panics at `parse_ok` expect (line 31) because finish returns `Usage`; other 25 tests pass.
- `total >`→`>=` (src/dsh.rs:436): `exact_total_byte_limit...` panics at feed `.expect` (line 838) at the exact 511-byte limit; other 25 pass.
- open-step `turn_end` guard removed (src/dsh.rs:887): `turn_end_with_an_open_step...` panics at `unwrap_err` (line 866) because feed returns `Ok`; other 25 pass.

## Attacks tried (including ones that found nothing)

1. **Run/reproduce**: baseline executed clean in 8.4 s; source manifest enforced before and after; `target/` excluded from manifest; workspace unchanged by mutations (harness hashes source/tests each case).
2. **S1–S3 mapping**: each SPEC scenario maps to exactly one named test and one non-equivalent mutant. S1 boundary 5==5; S2 exact 511-byte fixture with 1/2/7/511 chunking and `limit-1` rejection + Poisoned; S3 event=4 message + subsequent Poisoned feed/finish. All verified at their real assertions.
3. **S4 comment**: comment at src/dsh.rs:38–43 matches code — unknown type → `UnsupportedEvent`, `deny_unknown_fields` on validated records, opaque thinking/tool/tool_call/tool_result/trace only increment counts and retain no payload. No finding.
4. **Chunk/field boundaries**: 511-byte fixture count checked; `bytes_received` exact; no-newline tail covered by existing test; CRLF exact `max_line_bytes` covered (tests/dsh_adapter.rs:563). Exact LF line content followed later by newline **not** covered — fresh attack below.
5. **Fresh non-equivalent mutant**: src/dsh.rs:613 `self.line.len() > max_line_bytes` → `>=`. Full suite: **26/26 dsh_adapter tests pass (survivor)**. Divergence proven in scratch copies with `DshLimits{max_line_bytes:5,..}`: original `feed(b"12345")` = Ok; mutant = `LineBytesLimit{limit:5}`. This is a real chunk-boundary behavior difference, not equivalent.
6. **Checker fail-closed controls**: (a) tampered workspace source → `RuntimeError: source manifest mismatch`, exit 1 before any layer; (b) rewritten mutation to an equivalent no-op → `RuntimeError: reasoning-equality: did not demonstrate a behavioral test failure`, exit 1. Both controls went red, so they are non-vacuous. The mutation checker additionally requires exit 101, the named test in `... FAILED`, `test result: FAILED.`, and absence of `could not compile`.
7. **Unsuccessful/no-finding attacks**: shared mutation target reuse (defeated by isolated reruns); `max_events` exact-1 acceptance (implicitly exercised: first record accepted, second rejected); `end_line` `>`→`>=` would be killed by the CRLF exact test; `tree_sha256` derivation (recomputed = declared); output path escape guard and tests-hash guard present.

## Findings

- **F-R2-1 (missing coverage, boundary)**: no test pins acceptance of a plain-LF record whose content is exactly `max_line_bytes` when the newline arrives in a later feed. Mutation at src/dsh.rs:613 survives all 57 tests. Code behavior appears correct per the doc comment (inclusive bound); the gap is test coverage, and the 3/3 mutation claim is therefore not evidence for this boundary. Suggested fix: one test feeding exactly `max_line_bytes` bytes then `b"\n"`, plus a mutant entry.
- No SPEC mapping overclaim found for S1–S4; no wrong-behavior defect observed in the reviewed parser paths.

## Untested limits (honest)

- Required `TASK-CONTRACT.md` absent → spec-vs-contract attack **not performed**; original user request/approved scope cannot be confirmed.
- No line/branch coverage threshold, no fuzz/proptest, no random-order test run, no minimum-Rust-version matrix.
- No independent human approval of SPEC (autonomous); no independent grading of this report.
- Only 3 builder-chosen mutants + 1 verifier mutant exercised; no mutation tool (cargo-mutants/mutmut absent).
- CRLF/UTF-8 splits and hostile-input classes sampled by existing tests, not exhaustively fuzzed.

## Scope-specific verdict

`blocked` for full protocol verification (required task contract missing; only the bounded named scope is reproduced). For the bounded scope: **named S1–S3 baseline and all three non-equivalent mutants reproduce; S4 comment accurate; checkers fail closed. One non-equivalent survivor (exact plain-LF line boundary) shows the mutation layer is not complete.** This is not an old-coder Tier 3 guarantee and not a closeable `passed` verdict.
