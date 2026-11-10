# F2 工作区与进程验收

结果：**通过本阶段验收**。实测 `rustc 1.95.0` / `cargo 1.95.0`，原生 Linux；`CC=/bin/false CXX=/bin/false`；系统 `git` 与 `bwrap 0.11.1`。

| 验收项 | 实测结果 |
| --- | --- |
| Tokio | Workspace 接入 `tokio 1.54.1`（macros/process/rt-multi-thread/sync/time） |
| Git Worktree | `GitWorktreeManager` 以 argv 调用 `git worktree add/remove`；根目录强制在主仓之外 |
| 跨 Task 隔离 | 不同 Assignment 得到独立 directory/branch；相对路径写入不进入主工作区 |
| Runtime supervisor | `ProcessSupervisor`：argv 启动、超时、取消、`env_clear` + 白名单、进程组 TERM/KILL |
| 假 Worker | 成功 exit 0、失败 exit 7、超时、取消后进程组无存活成员 |
| env 白名单 | 空白名单时父进程 `PATH` 不可见；`injected_env` 仍可注入 |
| 不能写主区 | Linux `bwrap`：主仓 `--ro-bind`，worktree `--bind`，`--unshare-net`；绝对路径写主仓失败且文件未改 |
| CC/CXX=false | `cargo test/clippy --workspace --all-targets --locked` 通过（26 tests） |
| fmt | `cargo fmt --all -- --check` 通过 |

证据：

- [f2-test.stdout.txt](../tests/evidence/f2-test.stdout.txt)
- [f2-clippy.stdout.txt](../tests/evidence/f2-clippy.stdout.txt)
- [f2-metadata-summary.json](../tests/evidence/f2-metadata-summary.json)
- [f2-runtime-worktree-deps.txt](../tests/evidence/f2-runtime-worktree-deps.txt)
- [f2-runtime-test.stdout.txt](../tests/evidence/f2-runtime-test.stdout.txt)

边界：尚无 OMP/DSH 真实 CLI、cgroup 内存/CPU 限额、MCP transport、Git 合并或完整编排循环。无 `bwrap` 的平台跳过 OS sandbox 测试，不能把“未启用 sandbox 的 supervisor”当作生产隔离。进程组清理依赖 Unix `process_group(0)` 与 `kill -PGID`；不凭 PID 单独认领恢复。`forge-fake-worker` 仅用于测试，不进入生产路径。
