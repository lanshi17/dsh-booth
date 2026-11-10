#!/usr/bin/env bash
# Real execution: build the binary and run it against the fixture service, the way a user would.
#
# The mock API serves the same routes with the same same-origin guard, so this checks the
# console end to end without a configured GitHub account. A terminal UI cannot run here, so the
# one-shot commands stand in for it; the TUI's own path is covered by the pty run in EVIDENCE.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

port=${SMOKE_PORT:-3899}
base="http://127.0.0.1:$port"
fail=0

# The layer exists to run the tree it is in. `cargo test` builds the test harness but not
# `target/debug/booth`, so a checkout that has only been tested holds a binary that is missing
# or older than the source: trusting it either fails the run on a clean tree or, worse, passes
# it against the previous build. Build it here — cargo's incremental check is cheap — and let a
# compile error fail the layer with the compiler's own message.
booth=./target/debug/booth
cargo build --quiet --bin booth || { echo "FAIL build (cargo build --bin booth)"; exit 1; }
[ -x "$booth" ] || { echo "FAIL build (no target/debug/booth)"; exit 1; }

# A listener already on the port would answer every check below, so the layer would report
# another process's answers as its own result — for example a concurrent checkout's smoke run.
# Keep the default port when it is free; otherwise take a private one and fail closed only when
# an explicit SMOKE_PORT is busy.
free_port() {
  python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}
listening() {
  # Loopback only: an HTTP proxy from the environment would answer (or fail) in the
  # fixture's place, so no probe may be routed through it.
  curl -s --noproxy '*' --max-time 1 -o /dev/null "$1/" 2>/dev/null
}
if listening "$base"; then
  if [ -n "${SMOKE_PORT:-}" ]; then
    echo "FAIL fixture (SMOKE_PORT=$SMOKE_PORT is already in use)"
    exit 1
  fi
  busy=$port
  port=$(free_port) || { echo "FAIL fixture (cannot pick a free port)"; exit 1; }
  base="http://127.0.0.1:$port"
  echo "note: port $busy is in use; using $port"
fi

python3 dev/mock-api.py "$port" >target/smoke-server.log 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null; wait "$server" 2>/dev/null' EXIT

# Wait for the fixture instead of sleeping a fixed time, and stop as soon as it exits: a fixture
# that cannot start fails this layer with its own log, not a page of unrelated command failures.
ready=0
for _ in $(seq 1 50); do
  jobs -rp | grep -qx "$server" || break
  if curl -sf --noproxy '*' --max-time 1 -H 'x-orchestrator-request: 1' "$base/orchestrator/api/state" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.1
done
if [ "$ready" -ne 1 ]; then
  echo "FAIL fixture (no answer on $base)"
  sed 's/^/    /' target/smoke-server.log 2>/dev/null | tail -5
  exit 1
fi

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

run "status"   "$booth" --url "$base" status
run "overview" "$booth" --url "$base" overview
run "tickets"  "$booth" --url "$base" tickets
run "agents"   "$booth" --url "$base" agents
run "decisions" "$booth" --url "$base" decisions
run "search"   "$booth" --url "$base" search retry
run "version"  "$booth" --version

# The fixture refuses a management request without the guard header, so an accepted stop proves
# the CLI sent it.
if out=$("$booth" --url "$base" stop t-1 2>&1); then
  echo "PASS stop (fixture answered)"
else
  echo "FAIL stop refused by the fixture"
  echo "$out" | head -5
  fail=1
fi

[ "$fail" -eq 0 ] && echo "every command ran against the fixture"
exit "$fail"
