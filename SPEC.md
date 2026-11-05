# SPEC — booth 0.1.0: 审查与首发

- Tier: **3**（公开首发的 CLI/TUI：涉及凭据处理、进程控制、文件写入；错误会导致用户机器上的服务失控或密钥泄露）
- 隔离机制：**分支 `release/0.1.0`**（worktree 会缺少 `superagent/node_modules` 与 `target/`，重装 pnpm 依赖代价高且与落地树不一致；分支在同一棵树上跑，EVIDENCE 里说明这一点）
- Setup plan:
  - 工具：`cargo-mutants 27.1.0`（变异测试）、`cargo-audit`（依赖漏洞）、`cargo-llvm-cov`（改动行覆盖率）——已装其一，其余按需装；版本记入 `tools/versions.txt`
  - Git：在 `release/0.1.0` 分支上按 SPEC → 每个 GREEN → REFACTOR 打检查点提交；合并回 master 时 fast-forward
  - Gauntlet 新增文件（按路径）：`tools/gauntlet.sh`（唯一入口）、`tools/mutants.sh`（变异层封装）、`tools/versions.txt`（工具版本）、`SPEC.md`、`EVIDENCE.md`
  - 新依赖：**无**（只用 std + 已声明的 crate）

## 背景：这次审查要回答的问题

用户要求“通过代码审查，随后可以推送发布第一版”。这不是加功能，而是判定 0.1.0 能否发布。
因此本次改动的范围被限定为**审查中确认的真实缺陷**，其余一律记录、不改。

## Scenarios

### 缺陷 A：多字节 id 让控制台 panic

已实测确认：`&id[..id.len().min(8)]` 在非 ASCII 的 id 上 panic（`byte index 8 is not a char boundary`）。
工单 id 与 trigger id 目前是十六进制，但 `intake` 的 trigger id 前缀来自配置文件、`follow.ticket` 来自 `booth-follow.json`（可由用户编辑）。

```gherkin
Feature: 短 id 的显示永不 panic
  Scenario: 非 ASCII 的 id 被截断时不 panic
    Given 一个 id "中文工单编号中文"（每个汉字 3 字节）
    When  控制台为它计算 8 字符的短形式
    Then  返回的字符串不超过 8 个字符，且不 panic
  Scenario: ASCII id 的短形式不变
    Given id "03b16192bb7f2ac0"
    Then  短形式仍然是 "03b16192"
```

### 缺陷 B：`booth stop` 可能杀掉无关进程

`read_pid` 只做整数解析，`alive` 只看 `/proc/<pid>` 是否存在。pid 文件被残留、或 pid 被系统复用后，`stop` 会向**任意进程**发 TERM/KILL。

```gherkin
Feature: 只停自己启动的服务
  Scenario: pid 文件指向一个不是 dsh 的进程
    Given pid 文件里是当前 shell 的 pid（一个 bash 进程）
    When  运行 `booth stop`
    Then  命令失败并说明该 pid 不是 booth 的服务，且该进程仍然活着
  Scenario: pid 文件指向真正的服务
    Then  仍然发 TERM，并在退出后删除 pid 文件
```

### 缺陷 C：`replies()` 每次轮询读完整 outbox 并重复解析

2 秒一次、文件无上限增长，长跑后退化为 O(历史) 的每次轮询。

```gherkin
Feature: outbox 只读新增部分
  Scenario: 文件里有 5000 条旧消息、新增 1 条
    When  轮询一次
    Then  只解析新增的那条，返回值只含它
  Scenario: 文件被截断或替换
    Then  偏移量回退到 0，仍然能读到新内容，不 panic
```

### 缺陷 D：`signal()` 与 pid 复用

`kill` 通过 `Command::new("kill")` 调用外部程序，失败时错误信息不含目标 pid 与信号。

```gherkin
Feature: 信号失败时说清楚
  Scenario: 目标进程已消失
    Then  错误里同时包含 pid 和信号名
```

### 缺陷 E：发布所需的元数据缺失

0.1.0 首发需要：`Cargo.toml` 的 description/keywords 与仓库一致、`--version` 可用、README 的安装步骤与二进制名一致、LICENSE 与仓库声明一致。

```gherkin
Feature: 首发元数据完整
  Scenario: 运行 `booth --version`
    Then  输出以 "booth 0.1.0" 开头
  Scenario: 检查 LICENSE
    Then  文件存在且首行为 GPL-3.0 的标题
```

## Must NOT

- 不改 `superagent/` 子模块（它是上游，版本由 submodule 固定）。
- 不改管理 API 的调用语义（`api.rs` 的路径、头、错误映射），除非缺陷直接在其中。
- 不删除、不弱化任何现有测试；`cargo test` 的最终数量不得少于当前 34 个。
- 不引入新 crate。
- 不改 TUI 的视觉（颜色、布局、快捷键语义），除缺陷 A 涉及的 id 截断。
- 不提交任何凭据；不把 `provider.json`、`.credentials.yaml`、`oauth-url.txt` 纳入版本控制。
- 不修改已发布的 git 历史。

## Revisions

- v1（初稿）：范围定为 A–E 五个缺陷 + 发布元数据。审查以“能否发布”为标准，不顺手重构。
- v2（供应链层触发）：`cargo audit` 报 4 条告警，全部来自传递依赖——
  `ratatui 0.29 → lru 0.12.5`（两条 unsound）与 `paste`（unmaintained），
  `serde_yaml → unsafe-libyaml`（unmaintained）。改为 `ratatui 0.30` + `serde_norway 0.9`。
  这不是新增功能依赖，是替换被标记的传递依赖；改动范围仍是 A–E + 发布元数据，
  但 Must NOT 里“不引入新 crate”按此解释：不增加**新的**依赖，允许替换已标记者。
  `cargo audit` 归零，81 个测试仍全过。
