#!/usr/bin/env bash
# Print the toolchain the gauntlet depends on, so a reader can reproduce the run.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
echo "rustc   $(rustc --version)"
echo "cargo   $(cargo --version)"
echo "mutants $(cargo mutants --version 2>/dev/null || echo 'not installed')"
echo "audit   $(cargo audit --version 2>/dev/null || echo 'not installed')"
echo "llvm-cov $(cargo llvm-cov --version 2>/dev/null || echo 'not installed')"
echo "python3 $(python3 --version)"
echo "tar     $(tar --version | head -1)"
