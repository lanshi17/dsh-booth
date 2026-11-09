# SPEC — OMP old-coder retrospective review

- Tier: 3, hostile-input parsing and public Rust API.
- Spec approval: not obtained (autonomous run). This retrospective review contract is not a human-approved preimplementation specification. Independent-verification confidence is correspondingly downgraded.
- Setup: isolated explicit source copy, because these project files are untracked and an ordinary Git worktree would omit them. No installs, new product dependencies, commits, pushes, or original source changes.
- Added audit files: this SPEC, TASK-CONTRACT.md, source-state.json, gauntlet.py, instructions/, output/, workspace/; reviewer may add harness/, mutants/, scripts/ and output evidence within this audit directory only. Frozen candidate files must remain unchanged; mutants are in disposable sibling copies.
- Version/source: exact file manifest and tree digest in source-state.json. Build/test in the stated copy, using a separate Cargo target path. No prior EVIDENCE, conversations, acceptance reports or historical failures in the blind phase.

## Acceptance scenarios and failure model

S1 Canonical usage: uncached 5, cache-read 20, cache-write 3, output 7, reasoning 4 yields canonical input 28 and total 35; reasoning is not charged again. Exact total-output is permitted when optional cache detail is unknown; unknown detail stays unknown.
S2 Completeness: two started steps, one with missing usage or unfinished at EOF, cannot publish complete whole-transcript totals. Per-field observed sums and coverage must agree; zero usage differs from unknown usage.
S3 Completion: exit 0 + final + valid session + all turns normally ended is necessary. Cancellation, malformed order, nonzero/unknown exit or truncation must not become approved/task success. CLI completion and billing/empirical correctness are distinct.
S4 Terminal parser failure: bad JSON/UTF8, duplicate keys or aliases, unknown schema, contradictory/negative/fractional/overflow counters and exceeded limits fail closed; any subsequent feed/finish cannot recover success. Sensitive payloads must not appear in the returned summary/error.
S5 Bounded streaming: configured total, line and event limits hold under arbitrary chunking, UTF8 boundaries, CRLF and EOF without LF. A valid within-limit stream remains valid under equivalent chunking. Recursive JSON cannot defeat limits or crash the process.
S6 Noninterference and public API: offline parser invokes no model/process/network/tool or environment access, produces no Observation/review approval, preserves existing workspace behaviors, and has no new Cargo dependencies/unsafe. Examine public API visibility/serialization and misleading provenance claims.

## Must NOT

M1 Do not modify the original project or candidate snapshot, even to make the review pass. Preserve failed probes and mutants. Scratch test/fixture/mutant files are audit artifacts only.
M2 Do not equate existing green tests or an agent's final message with a successful review. Confirm actual test/target paths, exact source state, environment and tool versions.
M3 Do not report coverage, fuzzing, mutation kills, random order, cross-version compatibility or independent human grading that were not executed. Missing tools are UNAVAILABLE; deterministic generation instead of proptest/fuzzing is SUBSTITUTED, never a pass for that layer.
M4 Do not add/install tooling/dependencies, weaken assertions or change frozen acceptance criteria. Any requirement/spec gap is a finding for the user; reviewer fixes nothing.

## Gauntlet and mapping

Initial entry point: python3 gauntlet.py output/blind-baseline . It verifies snapshot hashes and runs full tests, cargo check, Clippy and fmt offline, writing actual stdout/stderr/exit status. This is a baseline stack, not complete Tier 3 assurance.
Reviewer must add a persisted supplemental entry point for concrete adversarial probes and at least 3-5 actual independently chosen non-equivalent mutants, prove each diverges on a concrete input, run tests against each, preserve hashes/commands/outputs and all survivors. Deterministic generated/chunking properties may use existing serde_json and standard library only. Do not relabel manual checks as cargo-mutants or deterministic generation as proptest.
Coverage: cargo llvm-cov and llvm-tools are not currently installed. Mark UNAVAILABLE if still absent; do not fabricate a coverage percentage or install tools. Stable Rust cannot supply nightly shuffle; if not available mark UNAVAILABLE or explicitly SUBSTITUTED with a narrower repeat-run check. Dependency audit is N-A for no dependency changes; capability review is still required. Complexity review must name concrete functions and consequences rather than inventing a numeric standard.
Every S1-S6/M1-M4 row must map to a named check/probe/layer or explicit unverified/n-a reason in the final review. Attack surfaces: run, spec-vs-contract, tests, home-grown checkers, mapping in both directions. A custom checker needs an observed known-bad failure. Optional canary not requested.

## Review protocol

Read instructions/SKILL.md and instructions/verifier.md. First blind-record source state, commands, attack list including unsuccessful attacks, findings with path/line/trigger/impact/reproduction and proposed severity/classification. Write output/BLIND-REVIEW.md and output/findings.json. Do not read any prior acceptance/EVIDENCE or builder conversation. End phase after these are persisted. The parent will subsequently reveal prior EVIDENCE for append-only comparison; do not alter the blind record in that phase.
Human grading has not happened. Findings/severity are proposals backed by reproduction, not self-approval of fixes. Round cap 2; this is round 1. No round 2/fixes required just to complete a code review with findings.

## Revisions

Initial retrospective review contract; no implementation work authorized.
