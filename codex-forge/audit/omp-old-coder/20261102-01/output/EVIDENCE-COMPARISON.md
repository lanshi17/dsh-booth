# Phase 2 — blind results vs prior builder EVIDENCE

Append-only comparison. The blind record (`output/BLIND-REVIEW.md`, `output/findings.json`) was not edited. `blind-seal.json` hashes were re-checked against those files at the start of this phase:

| File | Seal | Actual | Status |
|---|---|---|---|
| output/BLIND-REVIEW.md | `28cb4f0a…c5acca` | `28cb4f0a…c5acca` | MATCH |
| output/findings.json | `11c3bc48…40fd6` | `11c3bc48…40fd6` | MATCH |

Source state remains `46d6c1801a41010f5bf5af249b0477c4dfdbd1fe6042e05c5c5fb52c6720c0c1` (frozen manifest; unchanged by this phase).

## What was revealed and authenticated

The six prior files were read only after the blind record was sealed. Four of them authenticate by content against `prior-acceptance.json.evidence_sha256`:

| Revealed file | SHA256 | Claimed evidence name | Result |
|---|---|---|---|
| prior-tests.stdout.txt | `b8afd7b9da13…` | `final-test.stdout.txt` | MATCH (renamed copy) |
| prior-independent.stdout.txt | `9655a0aa8a03…` | `independent-imported.stdout.txt` | MATCH (renamed copy) |
| prior-import-manifest.json | `d57a49e14b79…` | `import-manifest.json` | MATCH |
| prior-acceptance.json | `7e7bc58e1504…` | not listed in its own evidence map | N/A |

The other claimed evidence files (`task-events.jsonl`, `revision-r1-events.jsonl`, `final-checks.json`, etc.) were **not** revealed and could not be authenticated. Their absence is a limit of this comparison, not a mismatch.

## Import hashes vs frozen source

All seven prior-import hashes match `source-state.json` and the bytes on disk, file by file:

| File | Prior import = source-state = disk |
|---|---|
| src/dsh.rs | `75ad89fb705c…` MATCH |
| src/lib.rs | `43e16887e990…` MATCH |
| tests/dsh_adapter.rs | `b79c10a9d88f…` MATCH |
| protocol/DSH-ADAPTER.md | `f9bddcfa25e9…` MATCH |
| tests/fixtures/dsh-live-smoke.jsonl | `7bdac947d4dd…` MATCH |
| tests/fixtures/dsh-live-development-sanitized.jsonl | `680119fb7e6a…` MATCH |
| tests/fixtures/dsh-event-key-schema.json | `8db9ecb71df8…` MATCH |

No import-hash mismatch was found, so no mismatch is reported. `prior-import-manifest.json` contains no file absent from `source-state.json` and no altered hash.

## Prior claims checked against the frozen source / blind run

1. **Test counts.** Prior: `project_passed=54`, `existing_tests=31`, `adapter_tests=23`. `prior-tests.stdout.txt` contains exactly 54 `... ok` names (`1+4+23+11+15`), and my current inventory lists 54 tests, 23 of them in `tests/dsh_adapter.rs`, with name-set equality and no prior-only/current-only tests. My blind baseline independently ran those 54 tests: 54 passed. Claim consistent.
2. **Baseline layers.** Prior: clippy `-D warnings` PASS, fmt PASS. Blind run independently reproduced both exit 0, plus tests and `cargo check` exit 0. Claim consistent.
3. **Fixture totals.** Prior JSON: dev uncached 92,414 / cache-read 1,779,072 / all-input 1,871,486 / output 45,968 / total 1,917,454; smoke 34,756. Blind probe P17 reproduced these exact values (23/23 dev steps, 1/1 smoke step). Claim consistent.
4. **16 independent local probes.** `prior-independent.stdout.txt` shows 16 passed and its content hash matches the claimed `independent-imported.stdout.txt`. These are **not** part of the 54 project tests (current inventory does not contain their names) and I did **not** execute them. The file `prior-independent-tests.rs` is a 97-line external test harness; statically its assertions are consistent with the frozen source for the self-contained cases. Two tests (`real_full_development_transcript…`, `timed_out_real_revision_trace…`) depend on external `../task-events.jsonl` / `../revision-r1-events.jsonl`, which were not revealed; their pass claims cannot be reproduced from the frozen workspace alone.
5. **R1 timeout claim.** Prior says the real R1 log ends with `final` before `turn_end` and is rejected. The frozen parser does have that check (`src/dsh.rs:756-758`), but the R1 log was not revealed, so the specific event-356 rejection remains **unverified**, not contradicted.
6. **Acceptance status.** `ADAPTER_ACCEPTED_AFTER_PARENT_CORRECTION_EMPIRICAL_STUDY_NOT_RUN` is an acceptance decision, not an assurance-layer result. The 54-green + clippy/fmt evidence does not include coverage, mutation, property, randomized-order, or version-matrix layers, and my three surviving mutants show concrete gaps in the suite. Acceptance is therefore not contradicted on current behavior, but it is **not** equivalent to complete old-coder assurance.

## F1–F3: current defects or test gaps?

They are **demonstrated test gaps, not current code defects**:

- On every mutant divergence input, the frozen candidate produced the documented correct result (`OK Complete`; `OK bytes=35`; `Err(Protocol: turn_end while a step is open)`). Only the disposable mutant copies differed.
- m1/m2/m3 are plausible one-line regressions that the full 54-test suite does not kill. The gap is in tests/assurance, not in shipped behavior.
- F3 is the sharpest: deleting the guard changes the caller-visible contract from `Err` to `Ok(incomplete)`, yet no test exercises `turn_end` with an open step. It is still a hypothetical regression, not a defect in the frozen file.
- F4 is a **description** defect in the module doc comment (`src/dsh.rs:37-41`), not behavioral: opaque event payload fields are accepted while the comment says unknown event fields are rejected. `protocol/DSH-ADAPTER.md` states the opaque behavior correctly.

Human grading remains **not obtained**. These classifications are reviewer proposals.

## Review-tool and report limitations (disclosed, not repaired)

1. **Probe harness mostly prints, does not assert.** `harness/src/main.rs` P1–P21 and `harness/src/bin/limits.rs` print outcomes for manual reading; only the mutant probe and my interpretation supplied the pass/fail judgment. There is no persisted assertion-bearing re-run entry point. A behavior change could alter printed text without failing a check.
2. **P7 was invalid as a numeric-type probe.** P7 put `-1`, `1.5`, `true`, `"1"`, `2^64`, `1e400` into an **unknown field `x`** on `turn_start`. The first five were rejected by `deny_unknown_fields`, not by numeric-counter typing; only `1e400` failed as non-finite JSON. Therefore P7 does **not** independently evidence rejection of negative/fractional/boolean/string/oversized usage counters. The blind report's S4 wording overstates what P7 demonstrated. Correct coverage exists in `tests/dsh_adapter.rs::non_integer_and_null_usage_fields_are_rejected`, but I did not independently reproduce it in the blind pass.
3. **F4 probe path misnamed.** `output/findings.json` F4 `command` says `harness/src/bin/main.rs P3`; the actual P3 code is `harness/src/main.rs`. `harness/src/bin/` contains `stdin_probe.rs`, `limits.rs`, `mutant_probe.rs`. The F4 `file` field (`src/dsh.rs`) and line range are correct. The blind record is left unchanged; this comparison records the path error.
4. **No persisted supplemental entry point.** The SPEC required a persisted supplemental entry point covering adversarial probes and mutants. I created `harness/` binaries and `mutants/m{1,2,3}` with `mutants/results.json`, but no single re-runnable script. This gap is **unresolved**.
5. **Mutation execution evidence.** I ran each mutant's suite and recorded test counts plus a divergence probe output; I did not build an exact-output assertion harness. The parent's announced independent mutant re-runs with exact output checks were **not performed by me** and are not claimed here.
6. **Blind-seal timing.** At the first phase-2 check, `blind-seal.json` and `revealed/` were not yet present; a later check found them. The hashes above were verified after they appeared, before any comparison writing. No blind file was touched.
7. **Baseline ≠ assurance.** The 54 tests plus clippy/fmt establish a green baseline only. Coverage, fuzzing/property tests, randomized order, tool-based mutation, and the 1.85 version matrix remain UNAVAILABLE, as recorded in the blind report. The prior 16 probes are a separate, unexecuted-by-me log, not additional tests in the 54.
8. **Unexecuted prior claims.** R1's timeout log, the two external-fixture independent tests, and all `evidence_sha256` files not revealed remain unverified. No mismatch is asserted from their absence.

## Verdict of this comparison

Prior import hashes and the 54-test/clippy/fmt/fixture-total claims are consistent with the frozen source and the blind rerun. The prior 16 independent probes are authenticated as log content but were not executed by me. The prior acceptance does not cover mutation/boundary assurance; F1–F3 remain demonstrated test gaps, F4 remains a low-severity description mismatch. No prior claim was treated as a test I ran, and no missing assurance layer was converted into a pass.
