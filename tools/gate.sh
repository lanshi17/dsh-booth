#!/usr/bin/env bash
# The single entry point for booth's checks: build, tests, and the size gate.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

fail=0
step() {
  printf '\n=== %s ===\n' "$1"
}
record() {
  if [ "$2" -ne 0 ]; then
    printf 'FAIL %s\n' "$1"
    fail=1
  else
    printf 'PASS %s\n' "$1"
  fi
}

step "configure"
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Debug
record configure $?

step "build"
cmake --build build
record build $?

step "ctest"
ctest --test-dir build --output-on-failure
record tests $?

step "size gate"
# A file over the limit is where the next god object grows. src/ only.
limit=600
over=$(find src -type f \( -name '*.cpp' -o -name '*.hpp' -o -name '*.ts' \) -not -path '*/node_modules/*' -not -path '*/lib/*' \
  -exec awk -v limit="$limit" 'FNR > limit { print FILENAME ": " FNR; nextfile }' {} +)
if [ -n "$over" ]; then
  printf '%s\n' "$over"
  record size 1
else
  record size 0
fi

step "result"
if [ "$fail" -eq 0 ]; then
  echo "all layers passed"
else
  echo "a layer failed"
fi
exit "$fail"
