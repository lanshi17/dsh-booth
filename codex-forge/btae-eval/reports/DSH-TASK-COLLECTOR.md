# DSH isolated task collector

状态：**工程验收完成，正式 Pilot 未运行。** Pure Rust `collect-task` 将第 1 轮计划任务、公开种子和指定 arm SOP 导出到独立 Bubblewrap 工作区，调用已有有界 DSH supervisor，封存执行后快照，并由 `verify-task` 离线校验计划、输入、收据和产物摘要。默认网络 namespace 关闭；`--share-network` 只表示调用方明确授权的工程 smoke，没有 endpoint allowlist。Worker 无权产生隐藏测试或任务成功结论。

验收结果：85 项测试通过；Clippy `-D warnings`、格式检查、readiness 预期退出 1、CLI 收集/校验 smoke 通过；3/3 手工行为变异通过。验证入口为 `python3 audit/dsh-task-collector/20261103-01/verify.py output/replay`，输出目录必须新建，源码/fixture manifest 绑定于 `source-state.json`。最终验证源树摘要为 `32e94c2ffbd50878514bee1c2fb301c2861b2e83d8acb4a14eb26bbfec535392`。

新增边界包括：仅首轮导出（后续轮次拒绝）、公开种子哈希绑定、只读输入和可执行文件、项目/Home/bundle 重叠拒绝、显式 profile 文件、环境 allowlist、任务工作区与主源码隔离、超时/取消/坏 JSON/非零退出收据、create-once 输出、额外/删除/修改/符号链接产物拒绝、变更 SOP/manifest 拒绝。

真实 DSH 任务 wrapper 尝试返回 `worker_failed`，DSH 报告 `ark` 凭据在 wrapper 内不可用；该私有运行没有发布凭据或原始 stderr。不能据此声称真实任务完成；低层 `collect` 的既有独立 DSH smoke 仍成功。OMP fresh blind review 超时，仅有初始记录，无有效 verdict，状态保持 `not performed`。

尚未完成：正式五组 Pilot、跨轮轨迹恢复、Worktree DAG 调度、独立隐藏 evaluator、可信账单、cgroup/磁盘配额、endpoint allowlist 和模型不可变 revision 冻结。
后续更新（2026-11-03）：用户指定 commandcode 路由，连通性成功；工具任务仍失败，独立最小探测发现 DSH `prepare` runtime 错误。[当前状态与用法](DSH-COMMANDCODE.md)。上文 ark 尝试属于历史记录。
