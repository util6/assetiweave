# 交接记录：AGACP-05

## 基本信息

- Task ID：AGACP-05
- Commit：`230cec7`
- 执行日期：2026-09-07
- 基线 HEAD：`45d76d9`
- 工作区原有修改：无

## 1. 修改文件列表

| 文件 | 变更类型 | 说明 |
|---|---|---|
| `src-tauri/test-fixtures/fake-acp-agent.mjs` | 修改 | 增加 `getRecordEventCount` 事件计数辅助函数；支持 `model_discovery_error` 模式（Stage 1 成功，Stage 2 session/new 模拟 -32603 内部错误，拒绝 model 配置）；扩充 keepAlive 定时器保护挂起任务；支持 `prompt_error` 模式 |
| `src-tauri/src/backend/agent_market/runtime.rs` | 修改 | 新增 `acp_test_fixture_with_id`、`test_request`、`read_record_events`、`assert_clean_workspaces` 辅助函数；在 `AgentRuntimeManager` 最高层本地接缝落地 6 大端到端回归场景测试 |

## 2. 6 大端到端场景覆盖与验证

全部场景均在最高层 `AgentRuntimeManager`（包含真实 SQLite 数据库实例、`AgentInstallationRepository`、`AgentRuntimeManager` 启动与 reload、`AiExecutionBackend` / `AcpExecutionBackend`）上完整运行：

1. **Scene 1：正常 models + prompt** (`agacp_05_scene_1_normal_models_and_prompt`)
   - 验证健康探针返回 models 正常；
   - 验证 SQLite 安装记录 protocol=Ready, model=Ready, connected=true, execution_ready=true；
   - 验证显式指定模型 `fixture/model-accurate` 执行成功，返回文本 `"translated"`；
   - 验证 ACP 协议事件完整序列：`initialize -> new -> model (accurate) -> prompt -> close -> delete`；
   - 验证运行完毕后本地 workspace 干净清除。
2. **Scene 2：empty models + prompt 使用默认模型** (`agacp_05_scene_2_empty_models_and_default_model_prompt`)
   - 验证健康探针检测到模型列表为空，标记 `error_code="model_list_empty"`；
   - 验证 SQLite 安装记录 `protocol_status=Ready` 保持不变，`model_status="unsupported"`，`connected=true`, `execution_ready=true`；
   - 验证顺利进入动态 Registry 候选；
   - 验证默认模型执行成功返回文本 `"translated"`，且 record 中零 `model` 配置事件；
   - 验证 workspace 干净清理。
3. **Scene 3：model discovery error + protocol remains ready** (`agacp_05_scene_3_model_discovery_error_leaves_protocol_ready`)
   - 验证 Stage 1 连接探针成功，Stage 2 发现模型失败；
   - 验证 SQLite 安装记录保持 `protocol_status=Ready`，`model_status="failed"`，候选资格保持 `execution_ready=true`；
   - 验证 reload 进入 Registry 后默认模型执行成功；
   - 验证指定模型执行时被拒绝并返回 `AiExecutionError::ModelSelectionFailed`；
   - 验证 workspace 干净清理。
4. **Scene 4：auth error + clean process/workspace + no fallback** (`agacp_05_scene_4_auth_error_cleans_workspace_and_no_fallback`)
   - 验证健康探针返回 `available=false`, `error_code="auth_required"`；
   - 验证 SQLite 安装记录标记为 `protocol_status=AuthRequired`，`execution_ready=false`；
   - 验证被排除在 Registry 候选之外（候选数为 0）；
   - 验证执行该 agent 直接报 `AiExecutionError::Protocol` / `ProtocolDetail`（包含 authentication required），零 `prompt` 事件发送；
   - 验证进程被杀，workspace 干净清理，绝无 fallback 发生。
5. **Scene 5：prompt error、timeout、cancel、disconnect** (`agacp_05_scene_5_prompt_error_timeout_cancel_disconnect`)
   - 5a (Prompt Error): 验证 agent 报错 -32603 时返回 `AiExecutionError::Protocol` / `ProtocolDetail`；
   - 5b (Timeout): 验证在总超时后返回 `AiExecutionError::Timeout`；
   - 5c (Cancel): 验证轮询直到 agent in-flight 接收到 prompt 时调用 cancellation，agent 接收并记录 `cancel` 事件，返回 `AiExecutionError::Cancelled`；
   - 5d (Disconnect): 验证传输流断开或意外退出时返回 `AiExecutionError::Protocol` / `AgentExited`；
   - 所有子场景均断言无孤儿进程，workspace 完全清理。
6. **Scene 6：cleanup close/delete supported 与 unsupported** (`agacp_05_scene_6_cleanup_close_and_delete_supported_and_unsupported`)
   - 6a (Supported): 验证正常支持 close/delete 的 agent 触发 `close` 与 `delete` 事件，执行成功返回 `"translated"`；
   - 6b (Unsupported 无 fallback): 验证不支持 close/delete 且未配置 fallback 的 agent 执行结束时在 cleanup 阶段安全报错 `AiExecutionError::CleanupFailed { failures: ["delete_unsupported"] }`，无 `close`/`delete` 事件发出，子进程通过关闭 stdin 或 SIGTERM 优雅退出（捕获 `stdin_closed`/`sigterm`），本地工作区零残留。

## 3. 验证结果

| 命令 | PASS/FAIL | 摘要 |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | 代码格式化完全符合规范 |
| `CARGO_TARGET_DIR=target/test-target cargo test -p assetiweave --lib -- backend::agent_market::runtime::tests::agacp_05` | PASS | 6 个场景端到端测试全数通过 |
| `CARGO_TARGET_DIR=target/test-target cargo test -p assetiweave --lib -- backend::agent_market` | PASS | 45 个 agent_market 测试全数通过 |
| `CARGO_TARGET_DIR=target/test-target cargo test -p assetiweave --lib -- backend::ai_execution` | PASS | 75 个 ai_execution 测试全数通过 |

## 4. 边界与设计核对

- [x] 测试仅依赖操作系统临时目录、独立 SQLite 实例及 `test-fixtures/fake-acp-agent.mjs`
- [x] 绝不访问公网、真实 Google 登录凭据或任何用户主目录
- [x] 每一个测试用例执行完成后清理临时目录及对应工作区
- [x] 无孤儿进程、僵尸进程遗留
