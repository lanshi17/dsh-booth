# DSH live collector v1

Status: in-progress. `spec approval: not obtained (autonomous run)`. This task adds a bounded Rust process supervisor around the existing DSH JSONL parser. It does not add DSH SDK/Runtime dependencies and does not run a formal BTAE Pilot. A caller must supply OS isolation; the record says `network_isolation=caller_supplied`.

Acceptance: direct argv only; child environment is cleared and rebuilt from an explicit allowlist; stdout is incrementally parsed, stderr is drained separately; output, line/event, wall and cancellation limits stop the process group; malformed/nonzero/timeout/cancel runs never return a completed summary; successful runs return the existing `DshSummary` and `billing_verified=false`; optional output directory is create-once with `started.json` and `record.json` containing hashes and no prompt payload; existing output is rejected. Tests use a compiled fake process and cover success, environment clearing, exact stdout boundary, stderr flood, malformed protocol, timeout, cancellation and evidence collision.

Required gauntlet: full cargo tests, clippy `-D warnings`, fmt, readiness expected nonzero, and three persisted manual behavioral mutants. Coverage/proptest/random-order/min-version matrix unavailable; no full Tier 3 claim. Actual DSH help probe will be recorded separately from synthetic tests; no credentials are copied or printed.
