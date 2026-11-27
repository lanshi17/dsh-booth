#!/usr/bin/env bash
# Control-backed commands run through the private socket the kernel opens.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
output2=$(mktemp)
trap 'rm -rf "$state" "$output" "$output2"' EXIT
mkdir -p "$state/dsh/profiles/booth"
printf '%s\n' '{"browser":{"enabled":true,"headless":false},"computer":{"enabled":false}}' > "$state/features.json"
printf -- '- id: user-row\n  config:\n    keep: true\n' > "$state/dsh/profiles/booth/cordis.patch.yml"
printf '# Todos\n\n- [ ] imported task\n- [x] imported done\n- [ ] imported active (in progress)\n' > "$state/todos.md"
printf '#!/bin/sh\nprintf "# Todos\\n\\n- [ ] edited task\\n- [x] edited done\\n" > "$1"\n' > "$state/editor.sh"
chmod +x "$state/editor.sh"

( sleep 2.5; printf '/record\n'; sleep 1.5
  printf '/record\n'; sleep 1.5
  printf '/rename probe title\n'; sleep 1.5
  printf '/plugins enable @scope/tool\n'; sleep 1.5
  printf '/modelpreset save probe\n'; sleep 1.5
  printf '/modelpreset switch probe\n'; sleep 1.5
  printf '/compact\n'; sleep 1.5
  printf '/compact soft\n'; sleep 1.5
  printf '/compact snapcompact\n'; sleep 1.5
  printf '/goal budget 100\n'; sleep 1.5
  printf '/handoff\n'; sleep 1.5
  printf '/tools\n'; sleep 1.5
  printf '/jobs\n'; sleep 1.5
  printf '/jobs full\n'; sleep 1.5
  printf '/session info\n'; sleep 1.5
  printf '/session info extra\n'; sleep 1.2
  printf '/agents\n'; sleep 1.5
  printf '/shake\n'; sleep 1.5
  printf '/tree\n'; sleep 1.5
  printf '/fork\n'; sleep 2
  printf '/add-dir /tmp\n'; sleep 1.5
  printf '/dirs\n'; sleep 1.5
  printf '/remove-dir /tmp\n'; sleep 1.5
  printf '/fresh\n'; sleep 2
  printf '/move /tmp/moved\n'; sleep 2
  printf '/wt test\n'; sleep 2
  printf '/guided-goal ship it\n'; sleep 1.5
  printf '/btw what is this\n'; sleep 2
  printf '/tan background chore\n'; sleep 2
  printf '/loop 2 loopcount\n'; sleep 3
  printf '/delete\n'; sleep 6
  printf '/memory\n'; sleep 1.5
  printf '/marketplace list\n'; sleep 1.5
  printf '/cleanse check\n'; sleep 1.5
  printf '/security scan\n'; sleep 1.5
  printf '/prewalk mock/model\n'; sleep 1.5
  printf '/omfg stop repeating that behavior\n'; sleep 1.5
  printf '/advisor on\n'; sleep 1.5
  printf '/login\n'; sleep 1.5
  printf '/logout\n'; sleep 1.5
  printf '/browser status\n'; sleep 1.5
  printf '/computer status\n'; sleep 1.5
  printf '/plan-review\n'; sleep 1.5
  printf '/force read\n'; sleep 1.5
  printf '/skillful off\n'; sleep 1.5
  printf '/extended-context on\n'; sleep 1.5
  printf '/ssh list\n'; sleep 1.5
  printf '/todo import %s/todos.md\n' "$state"; sleep 1.5
  printf '/todo edit\n'; sleep 2
  printf '/todo append pty task\n'; sleep 1.5
  printf '/todo done pty\n'; sleep 1.5
  printf '/usage reset\n'; sleep 1
  printf '/status\n'; sleep 2
  printf '/changelog last 1\n'; sleep 1.5
  printf '\x04' ) |
  timeout 120 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$state" DSH_HOME="$state/dsh" EDITOR="$state/editor.sh" VISUAL="$state/editor.sh" \
    script -qec "stty rows 40 cols 120; $booth" /dev/null >"$output" 2>&1 || true

( sleep 2; printf '/goal show\n'; sleep 3; printf '\x04' ) |
  timeout 20 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$state" DSH_HOME="$state/dsh" \
    script -qec "stty rows 30 cols 100; $booth" /dev/null >"$output2" 2>&1 || true

grep -aq "renamed to probe title" "$output"
grep -aq "saved preset probe" "$output"
grep -aq "applied preset probe" "$output"
grep -aq "Compacted 3 history items" "$output"
grep -aq "compaction (soft): Compacted 3 history items" "$output"
grep -aq "snapcompact is unavailable" "$output"
grep -aq "goal budget is unavailable" "$output"
grep -aq "handoff checkpoint: Compacted 3 history items" "$output"
grep -aq "committed 3 todos" "$output"
grep -aq "imported active" "$state/todos.log"
grep -aq "edited task" "$state/todos.log"
grep -aq "Read a file" "$output"
grep -aq "job-1" "$output"
grep -aq "pty task" "$state/todos.log"
grep -aq "provider rate-limit reset" "$output"
grep -aq "finished" "$output"
grep -aq "s-1" "$output"
grep -aq "elided 2 tool results" "$output"
grep -aq "first question" "$output"
grep -aq "resumed forked title" "$output"
grep -aq "added directory" "$output"
grep -aq "workspace /tmp" "$output"
grep -aq "0 added directories" "$output"
grep -aq "refreshing session state" "$output"
grep -aq "resumed moved title" "$output"
grep -aq "resumed wt title" "$output"
grep -aq "recording screen:" "$output"
grep -aq "recording saved:" "$output"
grep -aq "memory local" "$output"
grep -aq "No marketplaces configured" "$output"
grep -aq "cleanse report" "$output"
grep -aq "Security scan started: scan-1" "$output"
grep -aq "Prewalk armed" "$output"
grep -aq "Rule added" "$output"
grep -aq "plugin @scope/tool enabled" "$output"
grep -aq "usage: /session" "$output"
grep -aq "Advisor enabled" "$output"
grep -aq "login requested" "$output"
grep -aq "Signed out of the DeepSeek account" "$output"
grep -aq "browser: on" "$output"
grep -aq "computer: off" "$output"
grep -aq "Plan review: add a verification step" "$output"
grep -aq "Force armed" "$output"
grep -aq "Skill listing disabled" "$output"
grep -aq "Extended context on" "$output"
grep -aq "No SSH hosts configured" "$output"
grep -aq "0.5.1" "$output"
grep -aq "guided goal interview started" "$output"
grep -aq "side answer" "$output"
grep -aq "background agent s-tan" "$output"
grep -aq "loop-2" "$output"
grep -aq "session s-2" "$output"
grep -aq "Objective: control parity" "$output2"
test "$(find "$state/recordings" -name '*.jsonl' | wc -l)" -ge 1
grep -aq '"type":"frame"' "$state"/recordings/*.jsonl
test -f "$state/dsh/profiles/booth/booth-control.mjs"
grep -aq "id: booth-control" "$state/dsh/profiles/booth/cordis.patch.yml"
grep -aq "id: webserver" "$state/dsh/profiles/booth/cordis.patch.yml"
grep -aq "id: browser-playwright" "$state/dsh/profiles/booth/cordis.patch.yml"
grep -aq "headless: false" "$state/dsh/profiles/booth/cordis.patch.yml"
grep -aq "id: user-row" "$state/dsh/profiles/booth/cordis.patch.yml"
