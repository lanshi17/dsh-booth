# EVIDENCE — booth 0.1.0 review (Tier 3)

- **Spec approval:** obtained. The user replied "批准，按这个范围做（推荐）" to the SPEC question.
  One revision was made afterwards (v2, the dependency swap) and is recorded in `SPEC.md`.
- **Later changes** (after 0.1.0 shipped): the shared-provider change and the coverage-ledger
  generator were reviewed with the same layers; their numbers are the run recorded below.
- **Source state:** `72cd975` on `release/0.1.0` (25 commits ahead of `origin/master`'s `33126f3`).
  Every number below comes from one `tools/gauntlet.sh` run against that tree, executed after the
  last code edit: exit 0, every layer `PASS`.
- **Toolchain:** `tools/versions.txt` (rustc 1.93.0, cargo-mutants 27.1.0, cargo-audit 0.22.2,
  cargo-llvm-cov 0.9.1, python 3.14.4, GNU tar 1.35).
- **Entry point:** `tools/gauntlet.sh` reruns every layer below (`tools/gauntlet.sh quick` for the
  fast ones). It reads the diff against `GAUNTLET_BASE` (default `master`).
- **Isolation:** the branch `release/0.1.0` in the user's own tree, not a worktree. A fresh
  worktree has no `superagent/node_modules` or `target/`, and rebuilding the bundle's pnpm tree
  would make the gauntlet's numbers about a different tree. The run therefore happens where the
  change lands. **Differing content:** none — the branch has no ignored file the trunk lacks.
- **Independent verification:** **not performed** (Tier 3 option, and `references/verifier.md` was
  never read; per its own rule the state is `not performed`, not `passed`). Confidence is
  correspondingly lower and rests on the layers below.

## What the review covered

The task was "通过代码审查，随后可以推送发布第一版". The SPEC therefore limited the change to
**defects the review confirmed**, not features:

| # | Defect | Why it mattered |
|---|---|---|
| A | `&id[..id.len().min(8)]` panicked on a non-ASCII id | the TUI crashes on data it accepts (verified with a reproduction before the fix) |
| B | `booth stop` signalled whatever held the pid in a stale pid file | SIGTERM/SIGKILL to an unrelated program |
| C | the 2 s poll reread and reparsed the whole outbox | O(history) work per tick on a file that only grows |
| D | a failed `kill` did not say which pid or signal | an operator cannot tell what failed |
| E | release metadata incomplete | `cargo publish`/first-release readiness |

## Spec → Test mapping

| Scenario | Test | Status |
|---|---|---|
| A: 非 ASCII 的 id 被截断时不 panic | `app::tests::a_short_id_never_panics_on_multibyte_text` | pass |
| A: ASCII id 的短形式不变 | same test (`"03b16192bb7f2ac0"` → `"03b16192"`) | pass |
| B: pid 文件指向一个不是 dsh 的进程 | `service::tests::stop_refuses_a_pid_file_pointing_at_another_program` | pass |
| B: 参数层面只认本 profile 的服务 | `service::tests::matches_only_this_profiles_service_command_line`, `is_service_recognizes_its_own_dsh_process_only` | pass |
| B: pid 文件指向真正的服务 | covered by real execution (the live service was stopped and restarted, see below) | pass |
| C: 5000 条旧消息 + 1 条新增 | `intake::tests::reads_only_what_was_appended` | pass |
| C: 文件被截断或替换 | `intake::tests::a_shrunk_outbox_starts_over_and_a_partial_line_waits`, `an_unreadable_outbox_reads_nothing_instead_of_failing` | pass |
| D: 目标进程已消失 | `service::tests::a_failed_signal_names_the_pid_and_the_signal` | pass |
| E: `booth --version` | `tools/smoke.sh` (real execution) | pass |
| E: LICENSE 存在且为 GPL-3.0 | read by eye: first line is `GNU GENERAL PUBLIC LICENSE`, `Cargo.toml` says `GPL-3.0-only` | pass |
| **Must NOT: 不改 superagent 子模块** | `git diff --submodule` against the merge base: no submodule entry | pass |
| **Must NOT: 不改管理 API 调用语义** | `api.rs` diff adds `put`/`save`/`settings`/`block`/`set` only; no route or header changed | pass |
| **Must NOT: 不删不弱化测试（≥34）** | 34 tests at the start, 97 at the end, none removed (0 deletions in test modules) | pass |
| **Must NOT: 不引入新 crate** | revised in SPEC v2: two transitive deps were *replaced* (see below); `cargo tree` shows no crate that was not there before except maintained equivalents | pass |
| **Must NOT: 不改 TUI 视觉** | `git diff` shows no colour, layout or keybinding constant changed; `app.rs` changes are tests | pass |
| **Must NOT: 不提交凭据** | `tools/secrets.sh` (with a negative control) | pass |
| **Must NOT: 不改已发布历史** | `git log --oneline origin/master..HEAD` lists only new commits; no force-push was made | pass |

## Gauntlet (final fresh run)

| Layer | Command | Result |
|---|---|---|
| Tests | `cargo test` | 97 passed, 0 failed |
| Types | `cargo check --all-targets` | 0 errors |
| Lint | `cargo clippy --all-targets -- -D warnings` | 0 warnings |
| Format | `cargo fmt --check` | clean |
| Changed-line coverage | `cargo llvm-cov` + `tools/changed_lines.py` | 1232/1255 changed executable lines, 23 listed exclusions, 0 unexplained |
| Mutation | `cargo mutants --in-diff` via `tools/mutants.sh` | 79 caught, 3 listed equivalents, 0 uncaught, 0 unviable-to-judge, 3 unviable |
| Property-based | not used | **N-A**: the changed code has no algebraic invariant; the parsers are pinned by cases instead |
| Real execution | `tools/smoke.sh` + a live service | every one-shot command ran against the fixture (exit 0); a live instruction was accepted, ran to `person / review` and opened PR #8, checked and then closed; `booth restart` stopped and started the real service |
| Supply chain | `cargo audit --deny warnings` | 0 vulnerabilities, 0 warnings |
| Suite health | `cargo test -- --test-threads=1`, then 3 repeats | **SUBSTITUTED**: no randomized-order runner for cargo; serial order plus repeats cannot detect whole-suite order dependence |

### Supply chain detail (SPEC v2)

`cargo audit` initially reported four warnings, all transitive:
`ratatui 0.29 → lru 0.12.5` (RUSTSEC-2026-0002, RUSTSEC-2026-0253, both *unsound*),
`paste 1.0.15` (RUSTSEC-2024-0436, unmaintained), and
`serde_yaml → unsafe-libyaml 0.2.11` (RUSTSEC-2024-0449, unmaintained).
`ratatui` is now 0.30 and `serde_norway` 0.9 replaces `serde_yaml`; both are the same API surface
for this code (no call site changed beyond the crate name). The audit is clean.

### Mutation detail

`cargo mutants --in-diff` against the trunk: **85 mutants, 79 caught, 0 timeout, 3 unviable, 3
survivors that are listed as equivalents** in `tools/mutant_equivalents.txt`:

| Survivor | Why it cannot be killed |
|---|---|
| `Tail::fresh -> Default::default()` | the body *is* `Self::default()`; the mutant is the same program |
| `setup: < with <=` on the deadline | equality needs two clock reads to agree to the nanosecond |
| `App::draw_composer -> ()` | draws into a ratatui `Frame`; only a pty observes it, and the pty run is below |

The summary fails on any survivor that is not listed and on a listed name that stops surviving,
so the list cannot rot into a blanket exemption.

## Layers not run as specified

- **N-A:** property-based tests — `N-A`, see above.
- **N-A:** a public API compatibility check — the crate is a binary, not a library.
- **UNAVAILABLE:** independent verification — see the header.
- **SUBSTITUTED:** suite health — serial order and repeats, not randomized order.
- **SUBSTITUTED:** the TUI itself — `tools/smoke.sh` exercises the one-shot commands; the
  interactive console was driven through a pty by hand (below) rather than in the suite. What that
  cannot detect: anything that only appears after a resize, or across a real terminal's quirks.

## Real execution detail

- `tools/smoke.sh`: `status`, `overview`, `tickets`, `agents`, `decisions`, `search`, `version`,
  `stop` all ran against `dev/mock-api.py`; exit 0.
- Live service (`~/.dsh/state/superagent`, profile `superagent`): `booth restart` stopped the old
  pid and started a new one; `booth status` reported `setup ok`; an instruction sent with
  `booth send` was accepted (`explicitRouted`), ran to `person / review`, and opened PR #8, which
  was read, found correct, then closed with its ticket stopped and the branch deleted.
- The pty run covered: the splash screen, `/status`, `/provider`, `/setup`, `/keys`, the fuzzy
  palette with a scrolled selection, `Ctrl+P`, `Ctrl+R` (including across a restart), `Ctrl+W`/`U`/`Y`,
  the OAuth modal with a real 100-character authorize URL (fully wrapped), and the
  `◇ agent · not taken` / `◈ ticket … · working` transcript lines.

## Honest notes

- **A defect was introduced by my own fix and caught by a later layer.** The first version of the
  pid check rejected the real service: the command line is `node <script> --profile X --patch <file>`,
  and both the script path and the `--patch` value were read as positional arguments. The unit
  tests had guessed the shape; the real-execution layer caught it, and the matcher was rewritten
  (positional-before-the-first-option is a subcommand) with a test built from the live `/proc`
  command line.
- **Three of my own test expectations were wrong on first run** (Delete semantics, palette
  completion, the login form's initial cursor). The tests were corrected to the behaviour rather
  than the behaviour to the tests; the distinction is recorded here because "the test failed so I
  changed it" is exactly the shape of a weakened test.
- **The first mutation layer was too broad to finish.** Mutating all of `app.rs` (7489 segments)
  exceeded any reasonable budget and reported "hit the time budget", which the script correctly
  treated as a failure rather than a pass. It was narrowed to `--in-diff`, which is the claim the
  layer is meant to support: the changed lines are noticed by tests.
- **Seventeen mutation survivors were worked down, not argued away.** The first diff-scoped run
  caught 68 and left 13; each survivor became a test (the retry's pause, the positive `/proc`
  case, the poll's reply message, `device_id`'s two paths, `LoginForm::value`, the history cap),
  a code change that removed an unreachable tail, or a listed equivalence above. Two rounds also
  exposed that a counter-based retry could be mutated into an infinite loop, so it is now a
  bounded range.
- **The coverage ledger is line-number sensitive** and reported three stale entries after edits;
  the stale check is what surfaced them, and they were re-mapped by hand rather than widened.
- **`booth stop` cannot be tested against a real `dsh` in the suite** — that would spawn the
  service. The matcher is tested directly and the live restart covered the rest.
- **Known limit (not a defect):** a judgement can take minutes; the console now waits instead of
  declaring failure, so a slow model looks like a slow console by design.
- **Structural blind spot:** nothing here is evidence about Windows (the process control is
  `/proc`-based) or about a service the operator started outside booth beyond the "defers to it"
  path.
---

## Evidence Report — providers shared with the profile (Tier 3)

- **Spec approval:** not obtained (this change came from a direct request — "供应商和dsh共用 不要额外配置" —
  and the same SPEC's Must NOT list still applies: no submodule change, no new tests removed).
- **Source state:** the run below is the last thing executed before the tag, on top of `24562cd`.
- **Entry point:** `tools/gauntlet.sh`, base = the last release tag (`v0.1.0`).

### What changed

`provider.json`, the generated `provider.yml` and the `--patch` overlay are gone. The catalog comes
from the service's `GET /models` (the same answer the web panel renders), and a change writes
`agent-default-model` or `llm-pi-ai.providers` into the profile layer DSH already composes. A key
becomes `refs.BOOTH_<ID>_KEY` in `~/.dsh/.credentials.yaml`. An earlier install is migrated once.

### Gauntlet (final fresh run, exit 0)

| Layer | Result |
|---|---|
| Tests | 131 passed, 0 failed |
| Format / types / lint | clean; clippy `-D warnings` |
| Changed-line coverage | 958/1007 changed executable lines, 49 listed exclusions, 0 unexplained |
| Mutation | 98 caught, 12 listed equivalents, 0 uncaught, 0 timed out |
| Supply chain | `cargo audit --deny warnings` clean |
| Suite health | serial order, then 3 repeats |
| Secrets | clean, with its negative control |
| Real execution | every one-shot command against the fixture; a live instruction accepted and turned into a ticket; the migration ran on the real profile |

### Honest notes

- **Four mutation survivors were real test gaps**, not equivalents: the picker's selection used
  tests whose model ids differed, so matching on the model alone agreed with matching on both; the
  CLI's view only executed, never asserted its output; `Catalog::current` and the migration note
  were reached only through their happy paths. Each got a test, verified by re-introducing the
  operator and watching it fail.
- **The ledger was being maintained by hand and had drifted** — on this branch, nine entries were
  stale and three misses unlisted after ordinary edits. It is now generated from the code
  (`tools/ledger.py`), and a miss in a function with no documented reason fails the layer.
- **Both diff-scoped layers compared against `master`, which on the trunk is empty**, so they
  passed by comparing nothing. They now base on the last release tag and fail closed on an empty
  diff — the kind of fail-open the gauntlet exists to prevent, caught in the gauntlet itself.
- **Known limit:** `draw_*` and the OAuth success branch remain pty-only; the pty run is recorded
  above but is a person's observation, not an automated layer.

---

## Evidence Report — the message results `/search` already returned (Tier 3)

- **Request:** the second self-iteration round — find a different real defect from PR #12's
  outbox reader, fix it, run the checks and open a separate pull request for review.
- **Source state:** `30279cb` (`origin/master`) plus this branch. No submodule change.
- **Entry point:** `tools/gauntlet.sh` with its default base, the last release tag (`v0.2.0`).

### The defect

The bundle's `GET /search` searches Triggers as well as tickets and threads, and returns those
message hits in `items`. `format::search` rendered only `tickets` and `threads`, and decided
"nothing matched" from those two alone. A query that matched only a message therefore reported
silence even though the service had answered with the hit; `--json` printed it, so the console
and the one-shot command disagreed about the same view.

One-route fixture serving the bundle's shape with one `items` entry and no tickets or threads,
`booth search 重试` on the baseline:

```text
search · 0 tickets · 0 threads
  · nothing matched
```

while the same command's `--json` output printed the item under `items` at the same time.

### What changed

- `SearchView.items` now decodes the fields the console draws (`SearchItem`: actor, human, `at`,
  preview, thread title, filter reason) and keeps every other served field in a flattened map, so
  `--json` still carries the whole view.
- `format::search` prints a `messages` section with time, actor (or source), preview, thread title
  and filter reason; its header counts the hits; "nothing matched" now requires all three lists
  to be empty. The console's `/search` runs the same formatter.
- The palette and the CLI's help name messages rather than decisions/triggers, which is what the
  route returns.

The same fixture and command after the change:

```text
search · 0 tickets · 0 threads · 1 messages

messages
11-07 03:42 操作员           请重试一次部署，上一次卡在导出了  · 重试部署
```

The capture is the command's real plain-text output with the terminal's default colours.
There is no layout or colour change to the console frame, so no TUI screenshot comparison
applies; the console path is the same `format::search`, and the new tests pin it.

### Gauntlet (final fresh run, exit 0)

| Layer | Result |
|---|---|
| Tests | 134 passed, 0 failed (131 before; three new tests) |
| Format / types / lint | clean; clippy `-D warnings` |
| Changed-line coverage | 99/99 changed executable lines covered, 0 exclusions |
| Mutation | 9 caught, 0 missed, 0 equivalents |
| Supply chain | `cargo audit --deny warnings` clean |
| Suite health | serial order, then 3 repeats |
| Secrets | clean, with its negative control |
| Real execution | every one-shot command against the fixture, including `search`; the before/after capture above |

The two per-diff ledgers (`tools/coverage_excludes.txt`, `tools/mutant_equivalents.txt`) were
regenerated for this change: neither has a miss or a survivor to explain, and the tools fail on
last change's entries once they stop being misses/survivors, so those were removed. Their
reasons remain in this file and in git history.

### Material limitations

- **A pre-existing test is host-dependent.** `intake::api_tests::setup_uses_a_capable_cli_installed_under_the_state_directory`
  assumes the host `gh` is older than 2.57; this machine has 2.63.2, so `gh::ensure` correctly
  prefers the system CLI and the test fails on unmodified `master`. Open PRs #11 and #12 fix that
  test. The gauntlet run above put an old `gh` first on `PATH` — the condition the test means to
  create — so every layer ran; this change does not touch that test or the outbox reader.
- The bundle paginates search items at 20 and the console does not ask for more; that limit is
  unchanged here.
- Only the fields the console draws are typed; everything else in a served item is preserved for
  `--json`, but a future view field still has to be added deliberately to be drawn.
- A person merges this PR; the agent did not enable auto-merge.
