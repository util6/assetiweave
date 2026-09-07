# 交接记录：AGACP-01

## 基本信息

- Task ID：AGACP-01
- Commit：`3a0e4d6`
- 执行日期：2026-09-07
- 基线 HEAD：`6048bed`
- 工作区原有修改：无

## 1. 修改文件列表

| 文件 | 修改目的 |
|---|---|
| `src-tauri/src/backend/ai_execution/backends/acp.rs` | 解耦 `run_connection_probe` 与 `parse_session_models`；探针允许 `delete_unsupported` 降级为 warning 不破坏 protocol |
| `src-tauri/src/backend/agent_market/runtime.rs` | `probe_acp_health` 拆为阶段1连接探针与阶段2模型发现；模型为空记为 `unsupported` 不改变 protocol ready；认证失败设为 `auth_required` |
| `src-tauri/src/backend/agent_market/types.rs` | `connected()` 与 `execution_ready()` 解耦 `model_status` 依赖 |
| `src-tauri/src/backend/agent_market/repository.rs` | 候选者查询移除 `AND (protocol != 'acp' OR model_status = 'ready')` 限制 |
| `src-tauri/test-fixtures/fake-acp-agent.mjs` | 支持 `--mode=` 命令行参数及 `initialize_auth_error`/`auth_error` 模式 |
| `src-tauri/src/backend/agent_market/lifecycle/mod.rs` | 故障恢复测试用例中将断言失败的注入模式从 `no_models` 改为 `initialize_error`（经用户决策批准扩展白名单） |
| `agent-docs/feature-plans/antigravity-official-acp/07-progress.md` | 更新 AGACP-01 状态为 PASS 并记录偏差决策 |

## 2. 测试先行证据

- 新增/修改测试：
  - `src-tauri/src/backend/ai_execution/backends/acp.rs`: `life_03_connection_probe_succeeds_even_with_empty_model_list`
  - `src-tauri/src/backend/ai_execution/backends/acp.rs`: `life_06_connection_probe_succeeds_when_delete_is_unsupported`
  - `src-tauri/src/backend/agent_market/types.rs`: `readiness_separates_installed_connected_and_execution_ready`
  - `src-tauri/src/backend/agent_market/runtime.rs`: `acp_health_probe_succeeds_when_models_empty_and_leaves_protocol_ready`
  - `src-tauri/src/backend/agent_market/runtime.rs`: `acp_health_probe_marks_auth_required_when_auth_error`
  - `src-tauri/src/backend/agent_market/runtime.rs`: `acp_health_probe_marks_failed_when_connection_fails`
- 施工前失败命令：`cargo test -p assetiweave --lib -- backend::ai_execution::backends::acp::tests::life_03_`
- 精确失败原因：`check_connection` 内部调用 `parse_session_models`，在模型为空时直接返回 `session_model_catalog_empty` 错误。
- 实现后通过命令：`cargo test -p assetiweave --lib -- backend::agent_market::runtime::tests backend::agent_market::types::tests backend::agent_market::repository::tests backend::ai_execution::backends::acp::tests::life_ backend::agent_market::lifecycle::tests::agent_market_lifecycle_e2e_install_update_failure_recovery_and_cancel` (23 passed, 0 failed)

## 3. 验证结果

| 命令 | PASS/FAIL | 摘要 |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | 代码格式化检查通过 |
| `cargo test -p assetiweave --lib -- backend::agent_market::runtime::tests` | PASS | 6 passed, 0 failed |
| `cargo test -p assetiweave --lib -- backend::agent_market::types::tests` | PASS | 9 passed, 0 failed |
| `cargo test -p assetiweave --lib -- backend::agent_market::repository::tests` | PASS | 1 passed, 0 failed |
| `cargo test -p assetiweave --lib -- backend::ai_execution::backends::acp::tests::life_` | PASS | 6 passed, 0 failed |
| `cargo test -p assetiweave --lib -- backend::agent_market::lifecycle::tests::agent_market_lifecycle_e2e_install_update_failure_recovery_and_cancel` | PASS | 1 passed, 0 failed |
| `pnpm typecheck && pnpm test` | PASS | 前端 144 个测试文件全部通过 (711 passed) |
| `go vet -C cli ./... && go test -C cli -race ./...` | PASS | Go CLI 静态检查与竞态测试全部通过 |

## 4. 契约核对

- [x] logical ID 仍为 `antigravity`
- [x] 无 AntiGravity-specific ACP backend/protocol 分支
- [x] 无 `agy` fallback
- [x] model 状态未污染 protocol/readiness
- [x] 旧 Native installation 不进入 Registry (AGACP-02 目标)
- [x] Binary SHA-256 来自真实 archive
- [x] Conversation Adapter 无改动
- [x] Target Profile 无改动
- [x] Native 通用 seam 保留
- [x] 未验证能力没有写 passed

## 5. 真实 AntiGravity ACP E2E

- 状态：NOT_RUN（保留在 AGACP-06）

## 6. 未完成或偏差

- 偏差：`src-tauri/src/backend/agent_market/lifecycle/mod.rs` 中原有旧用例将 `no_models` 假定为协议失败，经用户交互确认将注入参数调整为 `initialize_error`，保持生命周期整体覆盖同时对齐 C-04 契约。
- 下一任务：AGACP-02（旧 Native installation 失配治理）。
