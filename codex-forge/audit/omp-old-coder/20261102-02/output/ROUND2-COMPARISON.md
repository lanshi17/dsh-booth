# ROUND 2 PHASE 2 — EVIDENCE COMPARISON (not a third review)

Scope limited to comparing the sealed blind record against the named draft EVIDENCE. No compilation, no test run, no candidate/checker modification, no commits/pushes/installs/model calls. `ROUND2-BLIND.md` and `round2-findings.json` were not modified.

## Integrity and late input

- Seal check: `sha256sum` matches `output/blind-seal.json` exactly — blind MD `981ae68…`, findings JSON `df2a0cf…`. Blind artifacts are unchanged.
- Manifest: `source-state.json` contains **exactly 53 files**; recomputed tree hash `eee74ab153cc739b0855e1613b19bc9c846ed0f83700d66abbc7cfb610554f70` equals the declared value.
- **Late-input limitation:** `TASK-CONTRACT.md` mtime is `2026-11-02 16:48:36 +0800`; the initial blind read at ~16:48:16 returned not found. The contract was first readable after the blind phase. Contract-vs-spec was therefore not part of the blind attack and cannot be retroactively restored. The check below is retrospective only.

## Claim comparison

| Claim (draft EVIDENCE / contract) | Draft value | Blind observation | Match |
|---|---|---|---|
| Source state | `eee74ab…` (summary, each mutant `source_sha256`) | same, manifest + recomputed tree | yes |
| Tests layer | exit 0 | exit 0; 57 passed, 0 failed | yes |
| Lint / format | exit 0 / exit 0 | exit 0 / exit 0 | yes |
| Readiness | exit 1 (expected) | exit 1 | yes |
| Manual mutants | 3/3 KILLED, exit 101, named tests | 3/3 independently rerun with isolated targets; same three tests and panic lines | yes |
| Mutant identity | `c540fc…`, `f049f9…`, `61358a…`; tests `7a56ba…` | same hashes observed | yes |
| Assurance scope | `full_old_coder_assurance: false`, `formal_pilot: NOT_RUN`, listed missing layers | blind independently listed the same limits plus a survivor | yes, no overclaim |
| Bounded test-gap claim | only 3 chosen mutants; no coverage/fuzz/random-order/min-Rust/approval | confirms; no broader mutation claim in draft | yes |

Mutant `manifest_path` values differ from the blind run (`0db19e70fb42` vs `895188b66901`) only because the run id is derived from the output directory; all hashes and outcomes agree. Not an evidence mismatch.

## Contract vs SPEC (retrospective)

The contract's bounded task — pin reasoning==output acceptance, inclusive exact total-byte limit under chunking, terminal rejection of `turn_end` with an open step, and clarify opaque payload non-validation — is exactly SPEC S1–S4. Its prohibitions (runtime logic, dependencies, datasets/readiness, existing behavioral assertions unchanged; no fixes/commits/push/installs/credentials/prior sessions/model calls) match SPEC's constraints and the phase rules. The autonomous no-approval statement matches SPEC. The three draft files contain no contradicting claim. This retrospective match does not repair the blind gap: the contract was unavailable while the blind verdict was formed.

## Findings

- **F-R2-1 remains open:** the blind fresh mutant at `src/dsh.rs:613` (`>`→`>=`) survives all 57 tests; divergence proven with `max_line_bytes=5, feed(b"12345")`. The draft EVIDENCE does not claim coverage of that boundary, so the bounded claim is accurate, but the gap is real. Builder plans a test after this phase.
- No discrepancy found between the named draft EVIDENCE and the reproduced numbers.
- No human SPEC approval and no human grading of this finding have occurred.

## Verdict

Bounded test-gap claims **match** the blind record; no global PASS is issued. Full independent verification remains incomplete (contract late, human grading absent, no coverage/fuzz/random-order/min-Rust layers). Any source changed after this phase is `not performed` at the two-round cap and cannot inherit this bounded result. Not an old-coder Tier 3 guarantee.
