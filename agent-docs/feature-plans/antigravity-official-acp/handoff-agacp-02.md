# 交接记录：AGACP-02

## 基本信息

- Task ID：AGACP-02
- Commit：`069f0ec`
- 执行日期：2026-09-07
- 基线 HEAD：`89f7e792`
- 工作区原有修改：无

## 1. 修改文件列表

| 文件 | 修改目的 |
|---|---|
| `src-tauri/src/backend/agent_market/types.rs` | `connected()` 增加对 `installation_status == Ready` 与 `protocol_status == Ready` 的严格校验，失配/incompatible 状态下断言为 false |
| `src-tauri/src/backend/agent_market/repository.rs` | 增加 `mark_incompatible(agent_id, error_code, error_message, checked_at)`，只更新状态与错误字段，不篡改原 program/protocol |
| `src-tauri/src/backend/agent_market/runtime.rs` | `recover_startup_with_catalog` 对比 catalog item，当已安装记录与 active catalog 协议或 distribution 失配时标记 `Incompatible`，并阻止其注册进动态 Registry |
| `src-tauri/src/backend/application/agent_market.rs` | `list_agent_market` 对 incompatible/失配记录强制 `update_available = false`；`preview_agent_installation` 拦截 `action == "update"` 并返回 `agent_reinstall_required`，放行 `reinstall` |
| `src-tauri/src/backend/agents/protocol/acp.rs` | 清理 unused imports |
| `src-tauri/src/backend/agents/registry.rs` | 清理 unused imports |
| `src-tauri/src/backend/ai_execution/error.rs` | 清理 unused imports |

## 2. 测试先行证据

- 新增/修改测试：
  - `src-tauri/src/backend/agent_market/types.rs`: `types::tests::readiness_separates_installed_connected_and_execution_ready`（验证 incompatible 记录下的 `connected == false`, `execution_ready == false`）
  - `src-tauri/src/backend/agent_market/runtime.rs`: `runtime::tests::startup_reconciliation_marks_mismatched_legacy_native_installation_incompatible_and_excludes_from_registry`（预置 `antigravity/native/system-antigravity/agy`，catalog 为 `acp/binary`，验证 startup 后 status 标记为 `incompatible` 且动态 Registry 中不包含该 agent）
  - `src-tauri/src/backend/application/agent_market.rs`: `application::agent_market::tests::incompatible_installation_denies_update_and_requires_reinstall`（验证 update 返回 `agent_reinstall_required`，reinstall 正常通过）
- 施工前失败命令：`cargo test -p assetiweave --lib -- backend::agent_market::runtime::tests::startup_reconciliation_marks_mismatched_legacy_native_installation_incompatible_and_excludes_from_registry`
- 精确失败原因：启动协调原本未按 active catalog 对比协议与 distribution，已安装记录仍被无条件判定为可进入 Registry 且状态未被标记为 `incompatible`。
- 实现后通过命令：`cargo test -p assetiweave --lib -- backend::agent_market` (39 passed, 0 failed) 与 `backend::application::agent_market::tests::incompatible_installation_denies_update_and_requires_reinstall` (1 passed, 0 failed)。

## 3. 验证结果

| 命令 | PASS/FAIL | 摘要 |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | 代码格式化检查通过 |
| `cargo test -p assetiweave --lib -- backend::agent_market` | PASS | 39 passed, 0 failed |
| `cargo test -p assetiweave --lib -- backend::application::agent_market::tests` | PASS | 1 passed, 0 failed |
| `go vet -C cli ./... && go test -C cli -race ./...` | PASS | Go CLI 静态检查与竞态测试全部通过 |
| `pnpm typecheck && pnpm test` | PASS | 前端类型检查与全部测试通过 |

## 4. 契约核对

- [x] logical ID 仍为 `antigravity`
- [x] 无 AntiGravity-specific ACP backend/protocol 分支
- [x] 无 `agy` fallback
- [x] model 状态未污染 protocol/readiness
- [x] 旧 Native installation 不进入 Registry
- [x] Binary SHA-256 来自真实 archive
- [x] Conversation Adapter 无改动
- [x] Target Profile 无改动
- [x] Native 通用 seam 保留
- [x] 未验证能力没有写 passed
- [x] SQLite schema 未做任何 migration，完全复用已有的 `incompatible` 状态
- [x] 旧 Native 记录保留外部原二进制与配置，不破坏 capability assignment

## 5. 真实 AntiGravity ACP E2E

- 状态：NOT_RUN（保留在 AGACP-06/07）

## 6. 未完成或偏差

- 无偏差。
- 下一任务：AGACP-03（Catalog 与证据迁移）。
