#!/usr/bin/env bash
# Print the toolchain the gauntlet depends on, so a reader can reproduce the run.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

fail=0

# version <label> <command...>
# Print the first line of a tool's version report. A tool that is absent or cannot answer is
# "not installed" and fails the report: the gauntlet cannot run the layer that needs it, and a
# blank line would read as a successful, empty version.
version() {
  label=$1
  shift
  first=
  if out=$("$@" 2>/dev/null); then
    first=${out%%$'\n'*}
  fi
  if [ -n "$first" ]; then
    printf '%s%s\n' "$label" "$first"
  else
    printf '%snot installed\n' "$label"
    fail=1
  fi
}

version "rustc   " rustc --version
version "cargo   " cargo --version
version "mutants " cargo mutants --version
version "audit   " cargo audit --version
version "llvm-cov " cargo llvm-cov --version
version "python3 " python3 --version
version "tar     " tar --version

exit "$fail"
