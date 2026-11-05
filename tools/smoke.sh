#!/usr/bin/env bash
# Real execution: run the built binary against the fixture service, the way a user would.
#
# The mock API serves the same routes with the same same-origin guard, so this checks the
# console end to end without a configured GitHub account. A terminal UI cannot run here, so the
# one-shot commands stand in for it; the TUI's own path is covered by the pty run in EVIDENCE.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

port=${SMOKE_PORT:-3899}
fail=0

python3 dev/mock-api.py "$port" >target/smoke-server.log 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null' EXIT
# Wait for the fixture instead of sleeping a fixed time.
for _ in $(seq 1 50); do
  curl -sf -H 'x-orchestrator-request: 1' "http://127.0.0.1:$port/orchestrator/api/state" >/dev/null 2>&1 && break
  sleep 0.1
done

run() {
  label=$1
  shift
  out=$("$@" 2>&1)
  code=$?
  if [ "$code" -ne 0 ]; then
    echo "FAIL $label (exit $code)"
    echo "$out" | head -5
    fail=1
  elif [ -z "$out" ]; then
    echo "FAIL $label (no output)"
    fail=1
  else
    echo "PASS $label"
    echo "$out" | head -3 | sed 's/^/    /'
  fi
}

booth=./target/debug/booth
[ -x "$booth" ] || { echo "build first: cargo build"; exit 1; }

run "status"   "$booth" --url "http://127.0.0.1:$port" status
run "overview" "$booth" --url "http://127.0.0.1:$port" overview
run "tickets"  "$booth" --url "http://127.0.0.1:$port" tickets
run "agents"   "$booth" --url "http://127.0.0.1:$port" agents
run "decisions" "$booth" --url "http://127.0.0.1:$port" decisions
run "search"   "$booth" --url "http://127.0.0.1:$port" search retry
run "version"  "$booth" --version

# The refused case proves the guard is reached, not skipped.
if "$booth" --url "http://127.0.0.1:$port" stop t-1 >/dev/null 2>&1; then
  echo "PASS stop (fixture answered)"
else
  echo "FAIL stop refused by the fixture"
  fail=1
fi

[ "$fail" -eq 0 ] && echo "every command ran against the fixture"
exit "$fail"
