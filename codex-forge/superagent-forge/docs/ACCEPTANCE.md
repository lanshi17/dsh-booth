# F0 架构与领域基线验收

结果：**通过本阶段验收**。实测 `rustc 1.95.0 (59807616e 2026-04-14)`、`cargo 1.95.0 (f2d3ce0bd 2026-03-21)`，原生 Linux 平台；未验证最低 Rust 版本或跨平台构建。

| 验收项 | 实测结果 |
| --- | --- |
| 独立 Workspace | 15 个 Rust 2024 crate |
| 构建 | cargo build --workspace --locked --offline 通过 |
| 测试 | 13 项通过，0 失败 |
| 代码检查 | Clippy -D warnings 与 fmt --check 通过 |
| CLI | --help、check、protocol 均通过 |
| 错误配置 | 并发数 0 返回 1；未实现的 run 命令返回 2 |
| 外部依赖来源 | 72 个解析包；本地仅这 15 个 crate，其余为 registry，无 Codex、外部路径或 Git 源 |
| 独立复制构建 | 复制到仓库外新目录、新 target，离线构建与配置检查通过 |
| C/C++ 编译器 | 独立构建令 CC/CXX=/bin/false，构建成功；未引入 cc/cmake/SQLite C 引擎依赖 |
| 旧 Fork | Codex Cargo.toml/Cargo.lock 与 HEAD 一致，之前 manifest 改动及文档已归档 |

测试覆盖：接受不授权；身份权限；Mandate 的后端白名单与到期；Worker 完成只进入待审；自审拒绝；审后才允许主控申请合并授权；错误 Worker/Assignment 拒绝；过期 HEAD 和失败测试不能批准；失败与拒绝后重试保留历史；旧报告和重复 Worktree 拒绝；取消后拒绝迟到报告；Reviewer 可依据失败测试拒绝任务；DAG 依赖、环与缺失依赖；配置拒绝未知权限字段。

第一轮完整 cargo metadata --offline 因未缓存的其他平台 js-sys 包返回 101；随后在不改变锁文件的前提下在线补齐并成功解析。它不影响此前通过的原生构建。全平台解析中出现的 wasm-bindgen/js-sys 是 Rust 平台绑定包，本次原生构建不运行 Node 或编译项目 JavaScript。

证据：[机器可读结果](acceptance.json)、[测试输出](../tests/evidence/test.stdout.txt)、[依赖清单](../tests/evidence/metadata-summary.json)、[独立构建日志](../tests/evidence/standalone-build.stderr.txt)。源码哈希清单排除了构建产物、验收日志和本验收文档，避免自引用。

边界：这是领域/架构基线验收。**尚无 Worker 进程执行、真实 Git Worktree 操作、OS sandbox、SQLite 后端、重启恢复、MCP transport 或 Git 合并**；这些不能计为功能通过。CC/CXX 验证也不表示无操作系统 libc 或未来引擎已经通过依赖审计。
