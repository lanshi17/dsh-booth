# 正式运行与证据收集（诊断运行器已实现，正式执行待实现）

用户已选：commandcode / deepseek/deepseek-v4.1-flash，五组统一 DSH headless。此前 DSH 文本/工具 smoke 成功不等于本 Benchmark runner 或 BTAE 六机制已经实现。

## 冻结清单

记录模型实际返回标识/不可变 revision（若供应商不可提供则明确研究降级）、temperature/top_p/seed 支持及压缩策略、DSH 可执行文件版本/内容哈希、脱敏 Profile 摘要、MCP 工具版本、镜像 digest、Python 版本、仓库 commit/文件摘要、任务/Prompt/机制代码摘要、价格生效日期。禁止将 API key 值写入配置、命令行或轨迹。

所有 arm 的框架 Profile 必须等价，仅干预附件和机制调度不同。外部 DSH CLI 是否需要 Node 属于 Worker 部署依赖；Rust 评分工具不依赖该 Runtime。主控不得借助同机全权限 DSH 自动去读取 evaluator-private 以便“完成任务”。

## 已实现：受隔离的假 Worker 诊断

Linux diagnose 从固定 plan 选择第1轮 task/arm/repeat，只按源文件清单导出公开仓库与 arm SOP，检查文件 SHA256。启动器使用 Bubblewrap 隔离 user/pid/network/mount namespace；仅导出 `/usr`、`/lib`、`/lib64` 运行依赖、单个 Worker 可执行文件、只读任务输入和可写的分配工作区。宿主 HOME、项目根、评估私有文件和环境变量不继承。命令使用 argv 数组和 stdin 文件，没有 sh -c。

主控监督：包含公开输入导出与代码快照清点的总 wall 时限、跨尝试累计 token 计数、每次尝试日志总字节上限、单条 JSONL 上限、最大尝试次数；只重试普通 Worker 非零退出。取消由宿主 cancel-file 发起；超时/取消终止进程组，Bubblewrap PID namespace 与 die-with-parent 覆盖孙进程和主控意外退出。没有自动从 PID 猜测恢复或重跑已中断任务。

每个输出目录仅创建一次。started.json 在导出/启动之前写入；setup 失败或主控中断时保留它，缺少 diagnostic.json 表示未完成，不等于代码失败。每次尝试保留原始 stdout/stderr、attempt.json 和完整 workspace-snapshot，重试沿实际 workspace 继续；最终 diagnostic.json 绑定 plan、manifest、可执行文件和产物摘要。verify-diagnostic 检查文件集合、内容摘要、快照、日志长度、计划身份、尝试序列、累计计数和最终状态一致性。这是可信宿主上的误改检测，不是签名，不能抵抗能重写整个证据包的恶意宿主。

预算时点：在冻结输入校验后、创建输出目录前启动 episode 时钟；截止判断持续到各次快照及最终产物清点完成，最终 JSON 凭据的写入耗时不在该已封存时点中。代码执行虽成功但封存前已超时，汇总仍标记 timed_out。

所有诊断均标记 runner_diagnostic_only，隐藏测试和真实任务成功值保持 null。fixture usage 是协议自报，不能视作服务商账单；超限检测可能在一次累计 usage 消息到达后才发现越界，记录实际越界数而不截成限额。将来真实模型必须另接可信 usage、最大生成参数和费用控制。

上述诊断路径仅覆盖 Linux 假 Worker。真实 DSH 的独立工程采集路径见下节；它尚未与该隔离运行器整合。资源 cgroup/磁盘配额、完整 seccomp、长期轨迹调度和正式结果收集仍待实现。

## 已实现：Pure Rust DSH headless 工程采集器

`src/dsh_live.rs` 提供 `collect` CLI 和 `run_live` API：固定 executable/argv、清空环境、显式环境 allowlist、非阻塞 stdin/stdout/stderr、进程组终止、超时/取消/输出上限、增量 `DshAdapter` 解析和 create-once 证据目录。它不会自行提供 bwrap/cgroup/endpoint allowlist；记录 `network_isolation=caller_supplied`，不生成正式 Observation，self-reported usage 始终 `billing_verified=false`。一次实际 DSH smoke 已完成，但仅作为工程连通性记录。

## 正式运行器待实施接口

1. 从 plan 中领取下一合法任务（同轨迹前一轮已封存）；校验所有摘要。
2. 仅导出该仓库公开文件、共同请求、所分配 arm SOP 到受限 Worktree/容器；不挂载本 checkout、Gold、隐藏测试、其他组或共享记忆。Git Worktree 只是组织隔离，不能替代 OS 边界。
3. 受控进程使用固定可执行文件+argv，stdin 传 prompt；不经 sh -c。捕获 JSONL、退出码、版本与实际账单，限制日志、进程组、网络和成本。
4. 主控封存 pre-edit 影响集，执行 arm 机制；每一重试关联同一 run 并消耗共同预算。机制辅助模型同型号同账本，不能引入免费的更强 Reviewer。
5. 封存轮末 snapshot，包括失败；后续轮次沿该实际 snapshot，不偷偷用参考修复救场。无法继续时保留原因与计划分母，不丢整条轨迹。
6. 将 snapshot 交独立 evaluator，运行冻结的隐藏检查，不向开发 Agent 反馈答案。开发中的 E Evidence Gate 只访问各组可见的公开条件。
7. 可信收集器产生 Observation，核对 plan/run/task/trajectory/model/config/artifact digest 一致性。原始失败结果不可覆盖，追加重试和裁决记录。

当前 CLI 提供 diagnose/verify-diagnostic 和工程 smoke collect，但仍不提供正式 execute/auto-analyze。score 接受 JSON 是数学接口，data_kind 只是来源标签而非认证；任何人都能构造它，不能把该标签当独立评估证明。bootstrap 接收聚合均值也不验证计划完整性，必须经过外部收集/配对/审计后使用。

## 成本事件

记录 input/output/cached_input（cache 是 input 子集）、工具数、尝试次数、wall 秒、参与者分钟、模型费用、计算费用、人类费用与价格版本。按 stage 分类但不重复相加；包含设计/检索/审批/验证、失败与取消。推理隐藏 token 和 provider usage 缺失单独标注，严禁估计为 0 后暗称实测。

每条评估 Observation 需能追溯到 run_id、冻结 plan、被测 snapshot 和 evaluator 测试版本。此版本样例中的 hidden_test_passed 是夹具常量，不是运行了隐藏测试。

2026-11-03 模型路由改为 commandcode，等价于用户 `/model commandcode/deepseek/deepseek-v4.1-flash`。首轮连通性成功；带工具任务仍有 DSH runtime 错误，见[模型配置](../config/README.md)和[本轮验证](../reports/DSH-COMMANDCODE.md)。旧 ark 验收与 manifest 绑定保持历史记录。

后续部署恢复：DSH 重复 dsh-tools 模块已去重，工具调用与真实首轮隔离任务完成，见[恢复验收](../reports/DSH-TOOLS-RECOVERY.md)。90 秒超时尝试同样保留。成功复测 token 用量超过草案预算，只能用于工程验收；正式 Pilot 门禁保持不变。
