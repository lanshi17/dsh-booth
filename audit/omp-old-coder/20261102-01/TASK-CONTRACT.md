# Task contract

User request: 调用 omp 进行 old-coder 代码审查。

Review subject: the current Pure Rust DSH headless JSONL offline adapter in btae-eval/src/dsh.rs, its module export, adapter tests, sanitized fixtures, and protocol/DSH-ADAPTER.md; inspect adjacent code only as needed to establish an actual impact. The project is an independent Rust 2024 workspace. DSH/OMP CLI tools are external tools, not product runtime dependencies. Review only; do not implement fixes, rewrite requirements, commit, push, install dependencies, read credentials, contact anyone, or run another model/agent. The reviewer must produce reproducible findings and honestly distinguish observed defects, spec gaps, missing assurance, and environmental limitations. Nothing in this review is a formal five-arm BTAE experiment.
