# DSH 工具运行时恢复：部署模块身份

2026-11-03，运行时源码基线 `4cc4e32df`。已定位并修复本机 DSH 0.2.1-rc.3 的重复包问题：顶层与两个内部消费者各加载独立 dsh-tools，导致调度器 Symbol 不相等。三份包自身的 28 个文件逐个 SHA256 一致；将两个嵌套目录备份到 node_modules 之外并改为指向顶层的相对链接后，模块身份一致。没有修改 Rust 代码、DSH JavaScript、依赖版本、模型路由或解析器。

[部署检查、修复与回滚](../config/DSH-TOOLS.md)包含可重复执行的模块身份探测，以及独立公开契约检查代码。[机器摘要](DSH-TOOLS-RECOVERY.json)记录源版本、模块文件集和快照摘要。原始安装身份探测退出 1；独立去重部署与修复后的本机安装均退出 0。两个部署的真实 bash/pwd 工具探测均退出 0、stderr 0。

| 工程任务尝试 | 结果 | 收据离线校验 |
| --- | --- | --- |
| queue-r01 / A / repeat 1，90 秒墙钟上限 | timed_out，90,012 ms | 退出 0 |
| 同一公开任务，新隔离目录，180 秒墙钟上限 | completed，103,341 ms，Worker 退出 0 | 退出 0 |

保留超时尝试；较长尝试是部署修复后的工程复测，不是增加正式实验预算后宣称方法胜出。成功尝试包含 12 个模型 step、15 次工具调用，stdout 46,198 字节、stderr 0。CLI 自报 input 117,080（其中 uncached 9,432、cache read 107,648）、output 6,649、total 123,729 tokens；cache-write/reasoning 未报告，billing_verified=false。**该 token 用量超过 Pilot 草案 60,000 预算，不能作为合规 Pilot 样本。** 现有 collector 未新增 token 硬配额。

对完成后的只读快照进行独立公开需求检查，69 条断言通过，覆盖 Job/Service/API 的 0..9 全部优先级、默认值、旧响应兼容、bool/非整数/越界拒绝及写入前验证。未读取 Worker 自建测试；同一检查对未实现优先级的原始 seed 退出 1。这不是独立专家 Ground Truth、隐藏评估或研究任务成功率。

当前 Rust 测试 85 项通过，cargo fmt 检查通过。正式 Pilot 未运行，readiness 声明、模型不可变版本和研究预算保持原样。本轮未新增独立 Agent 审查。仍待完成六机制忠实度、合格真值/隐藏 evaluator、跨轮恢复与 token 预算执行等工作；没有将本轮工程恢复记为研究完成。

本机安装备份位置和回滚步骤见部署文档。未来升级/重新安装可能重现重复布局；模块身份探测仅覆盖三个已知消费者。DSH error reason 的严格解析仍未扩展；其他错误流仍可能归为 protocol_error。所有原始模型输出、私有 session 和源码快照留在本机。
