# CI 验收规则

只有交付候选提交对应的 GitHub Actions `blocking-ci` 运行完成且 conclusion 为 `success`，才能标记 CI 合格。必须给出运行链接和完整提交 SHA；较早提交的成功、本地测试通过、已推送、queued、skipped 或 cancelled 都不能替代当前候选的远端成功。若候选提交继续变化，需重新验收。

## 当前合并门禁（本 fork）

`blocking-ci` **只**依赖：

- `forge-ci`（`btae-eval` + `superagent-forge`：Rust 1.95.0、仓库 fmt、Clippy `-D warnings`、全部目标测试、干净工作树；安装 Bubblewrap）
- `codespell`
- `cargo-deny`
- `blob-size-policy`

`CI required` 汇总作业要求上述依赖全部显式 `success`。上游 Codex 的 Bazel / `rust-ci` / SDK / `repo-checks` / postmerge / v8-canary **不再**进入合并门禁；需要时用 `workflow_dispatch` 手动跑。CI 不需要模型密钥，不执行付费模型调用。

## 历史说明

以下段落保留排查记录；其中“不删除原有跨平台任务”等表述已被本 fork 的窄门禁策略取代。

仓库 Prettier 检查覆盖 docs/active 文档。格式脚本仅将 audit/ 中的历史证据和 btae-eval/repositories 中的冻结样例排除在重写范围之外：前者按历史源码摘要留存，后者由 BTAE-Eval 测试校验 snapshot 清单。当前维护的 Rust 源码与 Python 工具仍接受格式检查，格式选择逻辑有正反例回归测试。

2026-11-04 排查发现：437dd3f98080107429537885638e2f18eea3d0c2 的 [blocking-ci](https://github.com/lanshi17/codex-forge/actions/runs/41614407840) 失败，包含真实格式问题与 GitHub 账户付款/支出限额导致的作业未启动。修复格式不代表账户问题已经解除。此修复以候选分支交付，所有必需检查通过前不合并到 main；不删除原有跨平台任务，也不将失败改成可忽略。

候选 84d48e44e71f28c5234871840060997f25d02564 的 [blocking-ci](https://github.com/lanshi17/codex-forge/actions/runs/41763111857) 已确认 repo-checks 和 superagent-forge 通过。BTAE-Eval 的畸形流测试因外层 500 毫秒耗时断言失败；后续修复让假 Worker 持续运行，改为验证协议错误、信号终止、管道关闭和任务期限内结束，避免将二进制哈希与共享 runner 调度当作性能门禁。修复候选 69bf40f7fde3b39f30c4411cd443d72eeaf83eb0 的[远端检查](https://github.com/lanshi17/codex-forge/actions/runs/41763744466)已确认两个独立 Workspace 和 repo-checks 通过，但当时仍存在 `Required runner group 'codex-forge-runners' not found` 及付款/支出限额两类启动阻塞。

运行器修复将 PR、合并后检查及发布工作流中的仓库专属私有组/标签替换为同平台、同架构的 GitHub 托管标签，见[运行器映射](../.github/workflows/README.md#hosted-runners)。无需创建 `codex-forge-runners`。Windows ARM64 的测试仍在原生 ARM64 上执行，归档仍在 x64 上构建；检查步骤、目标、分片及汇总门禁保持原样。

候选 3d8c4e63ca9b02848bfda8acddda13fb9c8d63e8 的[远端运行](https://github.com/lanshi17/codex-forge/actions/runs/41764809761)确认 Windows 和 SDK 已获得标准托管运行器，Python SDK 安装测试通过，缺失组导致的启动问题已解决。该轮最终 cancelled，不能据此宣告 CI 合格。Windows 构建另因缺少 MSVC 许可确认而失败；MODULE.bazel 要求由已接受条款的用户显式确认。后续配置仅在 Windows 上转发管理员显式设置的仓库变量 `BAZEL_MSVC_RUNTIME_VISUAL_STUDIO_EULA=1`，未设置时保持原有许可检查。

macOS 后续修复使用相同 ARM64 架构的标准 `macos-15` 替代收费 `macos-15-xlarge`，原有 Intel 选择及交叉编译目标不变。所有后续修复仍需新候选的远端检查通过；旧提交的成功、作业已启动或本地配置验证均不能替代完整 CI 验收。

用户已明确接受 Microsoft 条款并授权仅在 Windows CI 启用确认，仓库 Actions 变量已设置为 `1`。候选 2666f4578540f321e653e3df505414105f7b10b3 的[第 2 次运行](https://github.com/lanshi17/codex-forge/actions/runs/41796235406/attempts/2)中，Fence 和 Forge 检查通过，但共享 action 的许可步骤仍被跳过；后续改为工作流通过 `msvc-runtime-eula` 输入显式传值，并覆盖嵌套的 Windows lint 路径。未设置确认值时仍不导出许可变量。

同轮 BTAE 的 200 毫秒超时测试在准备阶段已耗尽总预算，因此没有启动 Worker。后续断言同时覆盖准备阶段零次执行和运行阶段一次执行，两者均不得重试；独立分类回归与后代进程测试分别验证准备超时，以及运行中的信号终止和管道关闭。准备、执行、封存继续计入同一个总预算。此处是 CI 回归修复，不构成正式 Pilot 实验结果。

候选 e877f37a84bacabb911478e0f34c539985987de7 的[远端运行](https://github.com/lanshi17/codex-forge/actions/runs/41801889312)确认 Windows 许可输入为 `1`，许可步骤实际执行且构建越过许可检查。随后 Windows 语音规则因缺少固定 `SystemRoot` 失败；普通托管运行器还缺少旧私有机器预装的 Cygwin/pkgconf 工具。后续 Windows CI 使用现有离线安装脚本和 `openai/codex` 的固定摘要公开归档配置工具，并显式传入仓库、系统目录及宿主架构。远程 Linux 执行不接收这些 Windows 参数。该轮 collector 回归也发现非超时场景误用了 80 毫秒期限，后续仅对挂起场景使用短期限，保留错误类型、退出状态、管道关闭和封存校验。

候选 80bc0a058c985ea9aeadf8c6018f51cf4ac788c1 的[远端运行](https://github.com/lanshi17/codex-forge/actions/runs/41804801667)中，BTAE、Forge、repo-checks、Fence、cargo-deny、格式及拼写检查通过，Windows 工具配置和原生依赖编译也已实际完成；但 Windows 运行时封存因 Bazel 状态文件缺少有效提交记录而失败，整轮仍不合格。后续 Windows 配置从 checkout 核验 SHA，显式选择原生状态脚本，并在大规模编译前构建轻量提交信息目标，验证 Bazel 文件中的 SHA。证据中的提交格式、唯一性和完整构建校验均保留。

同轮 GitHub annotations 明确记录 SDK 超过 20 分钟，Bazel 和跨平台 lint 超过 30 分钟的作业期限，当时仍在冷构建。后续将这些托管构建作业上限设为 120 分钟，测试自身的期限、目标、分片、断言和必需检查均保留。超时仍导致 CI 不合格，延长作业上限不代表本轮已通过。

候选 91b6741f821722ec70e89d36a3d0f8039f2861e3 的[远端运行](https://github.com/lanshi17/codex-forge/actions/runs/41809576857)已通过 Windows 提交信息预检、Windows argument-comment lint、两个独立 Workspace 和仓库检查。但 Windows 测试、Clippy 和发布构建均出现 GNU 原生库与 MSVC Rust 目标混链，整轮尚不合格。分片 1 还确认 `sqlx-macros` 宿主工具存在相同问题，说明仅恢复目标平台不足以解决混链。后续本地回退同时使用 gnullvm 宿主和目标，提供 Rust 1.95.0 宿主工具链及本地测试工具链，并让失败日志查询使用同一目标平台。远程 Linux 执行路径保留。

同轮 Linux 测试以及 macOS 发布构建、Clippy 明确出现磁盘耗尽。无远程凭据时禁用重复保存完整产物的 Bazel 磁盘缓存，保留声明输出和依赖下载缓存。一次性 GitHub 托管 Linux 机器清理未使用的 Android、.NET、Haskell SDK；macOS 仅清理未选中的 Xcode，保留当前选中安装及其别名。本机、自托管机器不执行清理，未能识别有效 Xcode 时直接失败。Linux Wine/PowerShell 超时已独立复现：单用例通过，而全套并发初始化 Wine prefix 超时。全套改为两线程并声明两 CPU 后，本地按最终 BUILD 配置实际通过 13 项，原有 60 秒目标期限与 PowerShell 30 秒期限保留，远端仍待验收。

工程 CI 合格与研究可以开跑是两个独立条件。即使 CI 通过，正式五组 Pilot 仍须通过方法忠实度、独立真值、隐藏评估、资源预算和环境冻结等 readiness 门禁，见[路线图](../btae-eval/protocol/ROADMAP.md)。

同轮 macOS PTY 高描述符测试在准备阶段触发 `EMFILE`；托管 macOS 配置仅提高内核文件数量上限，使隔离子进程可以实际打开 10240 以上的描述符，保留原有回退、进程组及父进程隔离断言。Linux rollout 所有权回归则排除 user namespace 中未映射的附加组 ID，并增加映射选择回归。CI 另在宿主真实组映射下执行同一所有权测试，必须实际通过一项且不得跳过，确保组保留行为确实被验证。

GitHub annotations 已确认该轮 SDK、Linux 发布构建及 macOS 测试再次超过 120 分钟总时限；Unix lint 也在同一时限被取消。相关预合并冷构建作业预算提高到 240 分钟，各单项测试期限不变。本地 Python 回归 86 项、rollout 全套 164 项及宿主所有权用例实际 1 项均通过，依赖锁检查通过。Windows gnullvm 单元测试目标已通过 Bazel 工具链解析；这不能替代 Windows 实际编译、执行及完整远端 CI。

候选 6f87ab1f2df7e0ce39da9b26c9a567f4994eda18 的[远端运行](https://github.com/lanshi17/codex-forge/actions/runs/41827593270)已通过两个独立 Workspace、repo-checks 和基础格式检查。SDK 与 Linux 发布构建下载固定 zlib 时遭遇 Ubuntu 快照服务器 HTTP 503。后续为原快照 URL 增加同版本 Launchpad 官方构建备用地址；x86_64 和 ARM64 文件均已实际下载，并分别匹配原固定摘要 `7074b6a2f6367a10d280c00a1cb02e74277709180bab4f2491a2f355ab2d6c20`、`cbe3d39ec32d3cc27c021ae4af11e7c67bdf9d700d573207e0941d4038056278`。独立空缓存 Bazel 拉取已实际复现快照 503 并成功下载两个架构，验证备用切换及摘要校验。不升级依赖、不移除校验；完整 CI 仍待验收。

编译动作级检查进一步发现：仅注册原生 gnullvm 工具链仍不足，通用 Windows 工具链优先级更高。后续优先注册具有 ABI 约束的工具链，并为自定义 stable/nightly 工具链显式设置对应通道条件，避免 stable 构建选到 MSVC 编译器发行包或 nightly 工具。stable 单元测试的动作查询现已确认所有 rustc 命令来自原生 gnullvm 1.95.0 发行包，宿主和目标均为 `x86_64-pc-windows-gnullvm`；nightly lint 动作也确认选择了独立的原生 gnullvm nightly 发行包；实际编译仍由远端 CI 验收。

候选 da6e97e1d5a84b2c3c0e384608d5ea01196f4e6b 的[远端运行](https://github.com/lanshi17/codex-forge/actions/runs/41832924693)已通过 Windows 测试分片 1/4、2/4、argument-comment lint、Clippy 和发布构建，确认修复越过实际编译及链接。分片 3/4、4/4 仍有失败：Git 子进程就绪超时、PowerShell parser 挂起及 ConPTY Ctrl-C 用例失败；TUI 重试日志另有动画帧快照差异和工作树转换超时。后续本地 Windows 回退恢复原跨平台配置的单线程测试限制，并限制同时运行的测试目标，不引入该配置的跳过列表。两个已挂起的 PowerShell parser 用例增加直接 stderr 阶段记录，若问题持续可区分启动、响应及子进程回收；仍要求解析结果为 `Unsupported`。Git 进程树夹具改为在各平台重新执行同一个 Rust 测试程序，去除 PowerShell 启动及控制台继承这一额外变量，保留就绪期限及后代存活断言。ConPTY 用例改为等待实际 ping 回复而非启动横幅中的地址；TUI 快照沿用状态效果测试的未来计时原点，在测试中固定首帧，保留原有符号、快照与布局断言。本地 Git 定向测试 3 项及最终 TUI 定向快照均通过；额外正向对照确认不执行进程组清理时，子进程会在包装进程退出后继续运行并写出失败标记，说明存活检查仍能捕获泄漏。这些修改仍需远端验收。

该轮 Blob policy 在 checkout 前因 Fence 受保护程序执行权限错误失败；失败运行器为 AWS Ubuntu 镜像 `20261018.96.1`，上一轮通过的运行器为 Azure 镜像 `20261101.360.1`。目前未确定根因，不能把平台差异当作已验证修复。保留 Fence 的固定版本及失败门禁，增加失败后的只读挂载和权限诊断；需要重跑验证启动、策略检查与 post-job 证据全部通过。整轮 CI 仍不合格。

候选 19cd6c10216d2c00e05e056c7f864b5bf90df8cb 的[远端运行](https://github.com/lanshi17/codex-forge/actions/runs/41856853135)中，Fence、Superagent Forge、repo-checks、格式及拼写已通过，但 BTAE-Eval 证据封存用例返回 `TimedOut`，未达到预期 `Completed`。该用例在启动 Worker 前同步写入启动日志，原一秒预算包含此 I/O；后续仅为这个封存/脱敏夹具设置十秒预算，保留完成状态、摘要、脱敏与目录冲突断言，80 毫秒超时用例不变。本地数据盘全套运行出现其他短预算 I/O 超时；在独立 tmpfs 测试目录中完整 86 项及 Clippy、格式检查通过。完整 CI 仍待新候选验收。

该候选的 Windows 分片 1、2、Clippy 和发布构建已通过，Git 进程树目标也已实际通过。分片 3 的沙箱回归发现公共可写目录中的工作区外文件和 `.git` 被删除；不能通过收紧测试目录权限规避这一失败。令牌实现将 Everyone 放入写入限制 SID 集合，使公共 ACL 可能满足限制检查。后续移除这一身份，保留能力、显式附加身份和 logon SID，并新增令牌 SID 回归；原删除回归显式安装公共可写 ACL，仍要求工作区/TEMP/TMP 可删除、外部文件和 `.git` 保持完整。隔离的 Windows 目标交叉编译和 Clippy 已覆盖令牌模块、测试及新增 ACL 片段，但尚未在 Windows 实际运行，不能据此宣布权限修复通过验收。

该候选的分片 4 最终确认三个失败：文件监听去抖用例在真实 25 毫秒等待后提前返回，PowerShell parser 目标在 named-block 用例处超过 300 秒，ConPTY 的 ping 在 Ctrl-C 后仍持续回复。后续去抖回归使用 Tokio 虚拟时间，保留原有提前触发检查和批次内容断言；仅测试依赖启用 `test-util`。PowerShell 的可用性探测与管道 AST parser 使用 `CREATE_NO_WINDOW`，避免附着调用方控制台。ConPTY 回归重新执行于独立控制台测试子进程，显式恢复可继承的 Ctrl-C 默认策略，不修改测试宿主的全局状态；等待 ping 的中断统计后才提交恢复命令，原有中断、恢复及正常退出断言保留。

本地文件监听、shell-command 和 PTY 三个 Linux 测试目标通过，文件监听和 shell-command 的库及测试二进制 Clippy 通过，Bazel 依赖锁检查通过。新增的 Windows 控制台调用片段与 PowerShell 命令构造函数通过 Windows 目标交叉编译和 Clippy；Windows PowerShell/ConPTY 行为仍待新候选的实际 CI 验收，不能把控制台继承假设当作已验证根因。

后续 Windows 分片 4 先运行 Git、PowerShell、PTY、Windows 沙箱和文件监听五个单元测试目标，再执行原完整分片；两次调用复用相同参数与本机测试缓存，完整目标清单和汇总门禁保持原样。这样可在编译完整 Agent/TUI 前暴露运行期失败，减少下一轮定位的等待。

配套复查还发现 NUL 设备 ACL 授权路径包含多余反斜杠。后续修正为 Win32 设备路径 `\\.\NUL`，并新增工作区可写、只读两种受限模式下的真实 `cmd.exe` 重定向回归：必须丢弃重定向内容、输出后续标记且退出码为零。设备授权实现通过 Windows 目标交叉编译和 Clippy，实际授权及新回归仍须 Windows CI 验证。

2026-11-05 已确认候选 `8459f226c76ffcd81f7f8e8c3f693b654d970850` 的[完整运行](https://github.com/lanshi17/codex-forge/actions/runs/41877081015)结束且失败。两个独立 Workspace、Fence、仓库检查、Windows 分片 1、Windows Clippy 和发布构建通过；其余平台仍有失败或超过 240 分钟被取消。NUL 重定向回归已在 Windows 通过；去掉 Everyone 后，原越界删除回归仍失败，另有沙箱子进程 `0xC0000142` 初始化失败。SID 集合测试通过并不能证明权限边界正确，这两项仍是验收阻断，不能宣称沙箱修复完成。

本轮后续修复包括：ConPTY 用例在发送 ping 前等待专用 PowerShell 提示符，中断后累计读取至下一次提示符，同时要求输出中存在 ping 统计，再执行恢复命令；UDS 的三处 PowerShell ACL 探针使用无控制台启动，并加入 Windows 前置回归目标。登录取消刷新测试参与已有认证环境互斥锁并清理外部访问令牌变量。Linux 代理夹具使用规范化后的 helper 路径启动并授权其父目录，避免旧版 bubblewrap 重新执行 Bazel 符号链接路径时找不到文件。SDK 的格式驱动测试明确断言冻结证据排除参数；归档比较夹具显式设置文件模式，保留内容与权限完全一致的断言。

Linux GNU 日志还确认大可执行文件跨卷复制、SQLite 写入和缓存封存均因磁盘耗尽失败；musl 的 V8 归档复制也返回 `ENOSPC`。守护进程生命周期夹具改为在临时目录所在卷准备共享可执行文件，并为各任务建立硬链接；活跃夹具以 `Arc` 持有它，最后一个夹具释放时清理，安装器真实复制测试继续保留。无远程执行密钥的托管 Unix 构建限制为四个构建动作、一个测试目标，普通 Rust 构建关闭 DWARF，不改变优化等级、调试断言或测试目标。发布归档的配置转换显式恢复行号调试信息；动作查询已确认普通构建的 `-Cdebuginfo=0` 之后有 `-Cdebuginfo=line-tables-only`，但这不是完整发布构建验收。

本轮本地验证：SDK 相关测试 83 项、Bazel 包装器 16 项（含新增托管/自托管、普通/发布配置组合）、登录 229 项、UDS 的 Unix 测试 5 项及相关 Clippy 通过；Linux 代理路由定向回归实际执行一项且通过。Linux 沙箱全套本机运行仍有四项失败，包含临时盘耗尽、网络命名空间及本机路径环境问题，不记为全套通过。守护进程 13 项首次也因本机 `/tmp` 空间不足失败；随后仅为该次测试绑定独立临时目录，全部 13 项通过。Windows 运行期修改仍须远端复测，既有 Windows 权限与初始化失败及 Unix 冷构建时限问题继续阻断整体交付。

候选 `eb591f6dc5bcb071f6b25dbbc7872eae4c442fa2` 的[远端运行](https://github.com/lanshi17/codex-forge/actions/runs/41989712079)继续暴露 Windows 越界删除及 Rust 子进程 `0xC0000142` 初始化失败；令牌 SID 测试通过仍不代表沙箱边界有效。macOS 参数注释 lint 在加载固定 toml2json 时收到 HTTP 502，尚未执行源码检查。Linux Clippy 在链接测试程序时耗尽磁盘，并非已确认的 lint 违规。

后续修复与诊断：节流测试也改用 Tokio 虚拟时间，原批次和提前触发断言保留。目录访问夹具在独立无控制台子进程中移除备份/还原特权，保留真实 PermissionDenied 与扫描结果断言；不再让目录扫描测试同时依赖生产沙箱启动。生产启动仍新增两种权限模式下执行实际 Rust 测试程序的回归。私有桌面启动路径使用实际窗口站名称，与 CreateDesktopW/OpenDesktopW 的所属站一致；这只是待 Windows 实测的初始化修复假设，不代表已确认 DLL 失败根因。

ACL 安装不再忽略 allow、deny-write 及工作区保护错误，底层 allow setter 的原生错误也向上传递；两个启动调用方在该失败路径关闭原始令牌句柄。新增原生令牌回归对公共 ACL 目录执行真实读取、写入、文件删除及目录删除，并记录独立 AccessCheck 结果以区分访问检查与 NTFS 删除行为。此回归尚待 Windows 执行，越界删除仍是阻断项，未收紧公共目录夹具或降低原断言。

Clippy 配置只请求 `clippy_checks` 输出，避免额外链接所有测试程序；目标清单、全部 lint 规则及独立完整测试/发布任务不变。本地文件监听单元测试通过，库与实际 `*-unit-tests-bin` 的 Clippy 通过；动作查询确认测试源码使用 `--test`、`--emit=metadata` 并产出 `.clippy.ok`。隔离 Windows 目标检查覆盖令牌测试及桌面名称查询，并通过 Clippy；它不等同于完整 Windows crate 编译或运行期验收。本轮 OMP old-coder 只读审查已完成；CommandCode 周额度不足，使用已授权的 DeepSeek 路由。审查建议作为待验证假设处理，未采用放宽桌面 ACL 或削弱删除检查的建议。正式 Pilot 继续延期，PR 保持草稿，完整 blocking-ci 成功前不合并。

候选 `badb600ece2c7d95e389a09b193348e533f75b1d` 的[远端运行](https://github.com/lanshi17/codex-forge/actions/runs/42006177009)已确认两个独立 Workspace、Fence、格式、拼写等检查通过，但仓库检查被 `docs/README.md` 表格格式阻断，cargo-deny 被新增 `RUSTSEC-2026-0441` 阻断。后续修正表格；核对该公告为 `pin-utils` 停维信息，`patched = []`，不报告已知漏洞。按现有逐项例外策略同步 deny/audit 配置，记录锁文件中 aws-smithy-http、aws-smithy-runtime、aws-smithy-types 和测试依赖 loom 四条直接路径；全部上游移除 pin-utils 后撤销例外。完整锁文件 cargo-audit 返回成功并保留 15 条既有警告；cargo-deny 0.19.0 的独立 pin-utils 夹具在移除例外时因该公告失败，使用当前配置时通过。完整工作区 cargo-deny 本机被未获取的 V8 ICU 子模块阻断，不记作通过。

同轮 Windows 前置检查中，Git、PowerShell、PTY 和文件监听目标通过；沙箱目标为 275 passed、5 failed、5 ignored，其中包含两项直接失败及后续互斥锁中毒导致的连带失败。目录扫描的独立子进程回归通过，生产 Rust 子进程仍返回 `0xC0000142`。独立原生 NTFS [诊断](https://github.com/lanshi17/codex-forge/actions/runs/42009045747)确认父目录 `FILE_DELETE_CHILD` 的 AccessCheck 返回允许，实际删除成功，而子文件 DELETE 检查返回拒绝。由此确认 WRITE_RESTRICTED 仍有父目录删除旁路，移除 Everyone 并不足够。

[完整限制令牌实验](https://github.com/lanshi17/codex-forge/actions/runs/42010478003)中，移除 WRITE_RESTRICTED 并给合成能力显式继承只读/执行权限后，11 项原生令牌测试通过。这仅证明该夹具上的删除阻断，尚未解决生产 legacy 全盘读取和启动需求，未纳入正式实现。[启动对照](https://github.com/lanshi17/codex-forge/actions/runs/42012758345)使用 MSVC 小程序，继承环境、仅 SystemRoot、空环境，以及默认/私有桌面与原始/受限令牌的十二种组合均以零状态退出；不能据此认定 SystemRoot 或桌面 ACL 是生产 GNU LLVM 初始化失败的根因。诊断工作流只在 `debug/windows-token-probe` 分支运行，不替换 PR 的完整工作流，也不构成 blocking-ci 验收。
