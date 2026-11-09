# DSH isolated task collector audit

This entry delivers an engineering-only first-round task collector. It exports a frozen public repository seed and one arm SOP into a private Bubblewrap workspace, runs a caller-selected executable through the bounded Rust DSH supervisor, snapshots the resulting workspace, and verifies the receipts offline. It does not run a formal Pilot or create an Observation.

Run from the repository root with a new destination:

```sh
python3 audit/dsh-task-collector/20261103-01/verify.py output/replay
```

The final local validation recorded 85 tests, Clippy, fmt, expected readiness rejection, fixture CLI collection/verification and 3/3 behavioral mutants. Its source/input SHA is in `source-state.json`; the summary is generated under ignored `output/final2/` during validation. A source guard rejects a known-bad digest, and mutation timeouts/compile failures fail closed. The entry point performs no model calls.

A private real DSH attempt under the same launcher returned `worker_failed` because DSH reported its `ark` credential was unavailable inside the task wrapper. No credential or raw stderr was published; the existing lower-level DSH live collector smoke remains the only successful real DSH connectivity result. This is a deployment integration limitation, not a Pilot result.

The OMP blind review attempt exceeded its deadline after writing an initial artifact and did not produce a verdict; `review-status.json` therefore says `not performed`. Coverage, fuzzing/properties, randomized order, minimum-version matrix, cgroup/disk quotas, endpoint allowlists and formal evaluator remain out of scope.
