# F5.2 远程 Push、OIDC 与签名授权验收

结果：**通过**。在 F5.1 持久化 merge auth 之上补齐 Ed25519 签名门禁、OIDC-profile HS256 JWT 身份，以及合并后 argv-only `git push`。

| 验收项 | 实测结果 |
| --- | --- |
| `forge-identity` | Ed25519 签名/验签 `MergeAuthClaims`；HS256 JWT（iss/aud/exp/sub） |
| 迁移 004 | `merge_authorizations.signature`；`schema_migrations` 含版本 1–4 |
| 签名门禁 | `TaskService::with_merge_keys` 签发；篡改签名 → `InvalidSignature`，不消耗授权前失败 |
| 远程 push | `push_branch` / `MergeExecuteRequest.remote`；bare remote tip 与合并 HEAD 一致 |
| MCP OIDC | `McpServer::with_oidc`；JWT 可 `initialize` + `forge_task_create` |
| CLI | 可选 `FORGE_OIDC_SECRET`/`ISSUER`/`AUDIENCE`/`SUBJECTS` |
| Workspace | `cargo test/clippy --workspace`（`CC=/bin/false`）通过 |

证据：

- [f5.2-test.stdout.txt](../tests/evidence/f5.2-test.stdout.txt)
- [f5.2-clippy.stdout.txt](../tests/evidence/f5.2-clippy.stdout.txt)
- [f5.2-metadata-summary.json](../tests/evidence/f5.2-metadata-summary.json)

边界：OIDC 为本地 HS256 配置子集，无网络 JWKS；push 不做 force、不经 shell。完整 IdP 联邦与凭据注入仍属部署侧。
