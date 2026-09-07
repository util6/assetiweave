# 交接记录：AGACP-04

## 基本信息

- Task ID：AGACP-04
- Commit：`45d76d9`
- 执行日期：2026-09-07
- 基线 HEAD：`62d1177`
- 工作区原有修改：无

## 1. 修改与删除文件列表

| 文件 | 变更类型 | 说明 |
|---|---|---|
| `src-tauri/src/backend/agents/registry.rs` | 修改 | 将 builtin registry 中的 `antigravity` 协议改为 `AgentProtocol::Acp`，默认命令调整为 `"antigravity-acp"`；移除 `model_discovery` 中的 `|| id == "antigravity"` 分支；更新 builtin registry 契约测试断言 |
| `src-tauri/src/backend/ai_execution/backends/native.rs` | 修改 | 移除 `use super::antigravity;` 与 `run_selected_native_execution`；将 `parse_agy_models` 重命名并重构为通用 `parse_native_models`（移除专有输出行过滤）；更新对应 TSV/Bare 单元测试 |
| `src-tauri/src/backend/ai_execution/backends/mod.rs` | 修改 | 移除 `mod antigravity;` 与 `mod antigravity_history;` |
| `src-tauri/src/backend/ai_execution/backends/antigravity.rs` | 删除 | 移除仅为 Direct-CLI production route 服务的专用 backend 实现 |
| `src-tauri/src/backend/ai_execution/backends/antigravity_history.rs` | 删除 | 移除仅为 Direct-CLI production route 服务的 history 解析转换 |
| `src-tauri/src/backend/ai_execution/backends/antigravity_tests.rs` | 删除 | 移除针对 Direct-CLI 专有 backend 的单元测试 |
| `src-tauri/test-fixtures/fake-antigravity-agent` | 删除 | 移除旧 Direct-CLI 测试可执行脚本 |
| `src-tauri/test-fixtures/fake-antigravity-agent.cmd` | 删除 | 移除旧 Direct-CLI Windows 测试脚本 |
| `src-tauri/test-fixtures/fake-antigravity-agent.mjs` | 删除 | 移除旧 Direct-CLI Node.js 测试逻辑 |
| `src-tauri/src/backend/ai_execution/executor.rs` | 修改 | 新增测试 `antigravity_routes_only_via_acp_and_rejects_missing_capabilities`，严格验证：1) 普通 antigravity 请求仅走 ACP 后端（Native 调用为 0）；2) 附带 team_tools 时因能力缺失直接返回 `TeamToolsUnavailable` 且不回退 Native；3) 附带 replay 时直接返回 `ResumeUnavailable` |

## 2. 测试先行与 TDD 证据

- 新增/修改测试：
  - `src-tauri/src/backend/agents/registry.rs`: `builtin_registry_contains_the_requested_acp_agent_definitions`
  - `src-tauri/src/backend/ai_execution/executor.rs`: `antigravity_routes_only_via_acp_and_rejects_missing_capabilities`
  - `src-tauri/src/backend/ai_execution/backends/native.rs`: `test_parse_native_models_tsv`, `test_parse_native_models_bare`
- 施工前失败命令（TDD Red）：
  - `cargo test -p assetiweave --lib -- backend::agents::registry::tests::builtin_registry_contains_the_requested_acp_agent_definitions`
  - 失败信息：`left: "agy" right: "antigravity-acp"`，断言 builtin fixture 仍为旧 native 模式。
- 实现后通过命令（TDD Green）：
  - `CARGO_TARGET_DIR=target/test-target cargo test -p assetiweave --lib -- backend::agents::registry` (7 passed, 0 failed)
  - `CARGO_TARGET_DIR=target/test-target cargo test -p assetiweave --lib -- backend::ai_execution -- --test-threads=4` (75 passed, 0 failed)
  - `pnpm typecheck && pnpm test frontend/src/components/settings/agentCatalog.test.ts` (PASS, 2 passed)
  - `go vet -C cli ./... && go test -C cli -race ./...` (PASS)

## 3. Source Guard 检查结果

- 执行命令：
  ```bash
  rg -n 'id\.as_str\(\) == "antigravity"|agent_id == "antigravity"|parse_agy_models' src-tauri/src/backend/ai_execution src-tauri/src/backend/agents
  ```
- 结果：Exit code 1（0 匹配，确保不再存在生产硬编码特殊分支与专有 parser）。

## 4. 契约核对

- [x] 所有 `antigravity` execution 只能按 ACP definition 进入通用 ACP backend
- [x] 彻底移除 Native backend 的 antigravity selector 及 `parse_agy_models`
- [x] 物理移除 6 个 Direct-CLI production 专属废弃文件
- [x] catalog capability false 时 Team 入口返回能力不满足错误（`TeamToolsUnavailable` / `ResumeUnavailable`），绝不调用旧 Direct-CLI fallback
- [x] 保留通用 Native backend、protocol enum 与其他 Native tests
- [x] Conversation Adapter 无改动
- [x] Target Profile 无改动
