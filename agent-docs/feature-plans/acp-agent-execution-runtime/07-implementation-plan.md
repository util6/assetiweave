# 实施计划：ACP Agent 执行运行时 (Implementation Plan)

| 字段 | 值 |
|---|---|
| 状态 | Complete |
| 任务原则 | 单任务 1–5 文件；每项独立验证；按依赖执行 |
| 主要规格 | `01-product-requirements.md`、`02-architecture-design.md`、`04-acp-process-runtime-design.md` |

## 1. 执行规则

1. 一次只执行一个 Task ID。
2. 开始前只加载该任务列出的文档段落和代码文件。
3. 先补失败测试，再实现。
4. 不顺手重构任务文件列表之外的模块。
5. 超过 5 个文件时停止并拆任务，不自行扩大。
6. 每项完成后记录验证命令和 PASS 证据。
7. Checkpoint 未通过不得进入下一 Phase。
8. commit message 使用中文 Conventional Commit。

## 2. 依赖图

```text
T01 SDK
 ├─> T06 Process
 └─> T08 ACP Connection -> T09 ACP Operations -> T11 ACP Backend

T02 Agent Types -> T03 Registry -> T12 Executor
T04 Execution Types/Error -> T10 Aggregator -> T11 ACP Backend -> T12 Executor
T05 Host Process Primitives -> T06 Managed Process -> T07 Process Tests -> T11

T12 -> T13 Runtime Injection -> T14 Translation Migration -> T15 Discovery
T13 -> T16 Task Registry -> T17 Tauri API -> T19 Frontend Provider -> T20 Cards
T14 -> T18 Engine Contract
T17 -> T21 Global Task / App Close
All -> T22 Final Verification
```

## 3. Phase 0：决策冻结

### T00 确认 Proposed 决策

**状态**：已完成（2026-08-13；采用全部推荐默认值，见 `09-progress.md`）

**目标**：在编码前确认索引第 5 节的六个问题。

**输入**：

- `SPEC_ ACP Agent Execution Runtime.md`
- `01-product-requirements.md` 第 7、24 节

**默认结论**：

- Gemini 暂留 legacy。
- Desktop 有 cancel + global running indicator。
- Translation no-tool。
- 空临时 workspace。
- Registry 不持久化。
- OpenCode Translation 直接切到 ACP；实现阶段不建立双路径 feature flag。

**验收标准**：

- [ ] 每项有明确 yes/no。
- [ ] 与推荐不同的决定已同步更新 01/02/05/07。

**验证命令**：文档评审，无代码。

**依赖任务**：无。

## 4. Phase 1：类型、Registry 与依赖

### T01 验证并引入 ACP SDK

**状态**：已完成（2026-08-13；`cargo check --manifest-path src-tauri/Cargo.toml` PASS）

**目标**：引入与 Rust toolchain 兼容的固定 ACP SDK 版本、Tokio I/O 兼容层和最小 Tokio features。

**先读**：

- `04-acp-process-runtime-design.md` §4.1
- AionCore `crates/aionui-ai-agent/Cargo.toml`

**涉及文件**：

- `src-tauri/Cargo.toml`
- `Cargo.lock`

**实施步骤**：

1. 添加固定 `agent-client-protocol` 版本。
2. 添加 `tokio-util/compat`，用于把 child stdio 适配到 SDK 的 futures I/O transport。
3. 为 Tokio 添加 `process/io-util/sync/macros` 等实际需要 features。
4. 不启用未使用的 ACP unstable feature。
5. 运行依赖解析和最小编译。

**验收标准**：

- [x] Cargo 能解析。
- [x] Rust 1.96.0 兼容。
- [x] 未开启 `tokio/full`，或已记录必要原因。
- [x] lockfile 由 Cargo 生成。

**验证命令**：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

**依赖任务**：T00。

### T02 定义 Agent 类型契约

**状态**：已完成

**目标**：建立 AgentId、AgentProtocol、AgentDefinition、probe/capability 类型及校验。

**先读**：

- `02-architecture-design.md` §5
- `03-aioncore-reference-code-map.md` §3–4

**涉及文件**：

- `src-tauri/src/backend/agents/mod.rs`
- `src-tauri/src/backend/agents/types.rs`
- `src-tauri/src/backend/mod.rs`
- `src-tauri/src/backend/agents/types.test.rs`（仅当项目允许独立 test 文件；否则 colocate）

**优先编写测试**：REG-03、REG-04、REG-05。

**验收标准**：

- [x] AgentId validation 固定。
- [x] Protocol 只有 Acp/Native。
- [x] Definition validation 拒绝空 command、非法 env key、非法 args NUL。
- [x] 类型不依赖 Translation 或 ACP SDK。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::types
```

**依赖任务**：T00。

### T03 实现 Builtin AgentRegistry

**状态**：已完成

**目标**：建立代码内置 Registry、OpenCode definition、lookup 和 duplicate fail-fast。

**先读**：

- `02-architecture-design.md` §6
- `03-aioncore-reference-code-map.md` §3–4

**涉及文件**：

- `src-tauri/src/backend/agents/registry.rs`
- `src-tauri/src/backend/agents/mod.rs`
- `src-tauri/src/backend/agents/types.rs`

**优先编写测试**：REG-01、REG-02、REG-06、REG-07。

**验收标准**：

- [x] builtin registry 唯一包含 Phase 1 OpenCode definition。
- [x] command=`opencode`，args=`["acp"]`。
- [x] lookup clone/borrow 语义明确。
- [x] duplicate ID 构造失败。
- [x] 无 SQLite dependency。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::registry
```

**依赖任务**：T02。

### T04 重构 AiExecution 类型与错误

**状态**：已完成

**目标**：把单文件 `ai_execution.rs` 拆为目录模块，先建立 request/result/limits/cancellation/error，不改变现有 CLI 行为。

**先读**：

- `01-product-requirements.md` FR-EXE、错误模型
- `02-architecture-design.md` §5、§10
- 当前 `backend/ai_execution.rs`

**涉及文件**：

- `src-tauri/src/backend/ai_execution/mod.rs`
- `src-tauri/src/backend/ai_execution/types.rs`
- `src-tauri/src/backend/ai_execution/error.rs`
- `src-tauri/src/backend/ai_execution.rs`（删除/迁移）
- `src-tauri/src/backend/mod.rs`

**优先编写测试**：EXE-01、model/prompt validation、error redaction。

**验收标准**：

- [x] `AiExecutionRequest/Result/Limits/Purpose/Cancellation` 可编译。
- [x] stable error code/view 映射存在。
- [x] 现有 public helper 暂时可 re-export，调用点不破坏。
- [x] prompt/result 不出现在 error Debug/Display 的公开 view。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution
cargo check --manifest-path src-tauri/Cargo.toml
```

**依赖任务**：T02。

### Checkpoint A：Foundation

```bash
cargo fmt --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml backend::agents
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution
```

Review：类型命名、OpenCode definition、SDK 版本。通过后进入 Phase 2。

## 5. Phase 2：进程 Runtime

### T05 提取可复用 Host Process primitives

**状态**：已完成

**目标**：在不改变现有一次性命令行为的前提下，公开长生命周期 process 需要的平台 helper。

**先读**：

- 当前 `backend/host_process.rs`
- `03-aioncore-reference-code-map.md` §6
- `04-acp-process-runtime-design.md` §3.7–3.9

**涉及文件**：

- `src-tauri/src/backend/host_process.rs`

**优先编写测试**：现有 timeout/process group tests 保持；新增 helper 幂等/平台测试。

**验收标准**：

- [x] 现有 `run_command_with_control` 行为零回归。
- [x] process group configure/terminate helper 可供 agents/process 使用。
- [x] 平台 `cfg` 边界清晰。
- [x] 不把 ACP 概念放入该文件。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::host_process
```

**依赖任务**：T00。

### T06 实现 ManagedAgentProcess

**状态**：已完成

**目标**：实现 async long-lived child、stdio transfer、stderr drain、exit watch、terminate/force kill/wait。

**先读**：

- `04-acp-process-runtime-design.md` §3
- AionCore `cli_process/mod.rs` 与 `spawn_sdk.rs` 指定 symbol

**涉及文件**：

- `src-tauri/src/backend/agents/process.rs`
- `src-tauri/src/backend/agents/mod.rs`
- `src-tauri/src/backend/host_process.rs`

**优先编写测试**：PROC-01..06、PROC-10、PROC-13、PROC-14。

**验收标准**：

- [x] stdio 只能 take 一次。
- [x] stderr bounded drain。
- [x] child exit 被 watch。
- [x] terminate 幂等。
- [x] safe spawn preview 不含 arg/env value。
- [x] 所有 spawn error path 尝试 cleanup。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::process
```

**依赖任务**：T01、T02、T05。

### T07 加固 Process Tree 测试

**状态**：已完成

**目标**：验证孙进程、launcher 提前退出和平台清理。

**先读**：

- `06-test-verification-acceptance.md` §5
- AionCore `force_kill_tree` 注释与测试

**涉及文件**：

- `src-tauri/src/backend/agents/process.rs`
- `src-tauri/src/backend/host_process.rs`

**优先编写测试**：PROC-07..09、PROC-15。

**验收标准**：

- [x] Unix grandchild 测试通过。
- [x] direct child 先退出仍清理 group。
- [x] Windows 对应测试或受控 cfg fixture 存在。
- [x] 每个 process test 自带 timeout。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::process -- --nocapture
```

**依赖任务**：T06。

### Checkpoint B：Process Safety

必须人工检查：

- 无 raw stderr info log；
- 无 process leak；
- 现有 ai_execution 一次性 CLI tests 仍通过。

## 6. Phase 3：ACP Protocol 与 Fake Agent

### T08 建立 ACP connection actor

**目标**：typed SDK connect、initialize、shutdown、event/disconnect channels。

**先读**：

- `04-acp-process-runtime-design.md` §4.2–4.5
- AionCore `AcpProtocol::connect`、`run_sdk_background`

**涉及文件**：

- `src-tauri/src/backend/agents/protocol/mod.rs`
- `src-tauri/src/backend/agents/protocol/acp.rs`
- `src-tauri/src/backend/agents/mod.rs`

**优先编写测试**：ACP-01..04、ACP-10、ACP-17、ACP-18。

**验收标准**：

- [ ] initialize local timeout。
- [ ] background actor 与 handle 分离。
- [ ] initialize response 缓存。
- [ ] client capabilities 不夸大。
- [ ] shutdown 幂等。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::protocol::acp
```

**依赖任务**：T01、T06。

### T09 实现 Phase 1 ACP methods 与 permission handler

**目标**：new/model/prompt/cancel/close 和最小 runtime event mapping。

**先读**：

- `04-acp-process-runtime-design.md` §4.6–5.3
- `03-aioncore-reference-code-map.md` §7–9、§12

**涉及文件**：

- `src-tauri/src/backend/agents/protocol/acp.rs`
- `src-tauri/src/backend/agents/protocol/mod.rs`

**优先编写测试**：ACP-05..09、ACP-11..16、ACP-19。

**验收标准**：

- [ ] typed text prompt。
- [ ] cancel 可在 prompt 中发送。
- [ ] model failure 不发送 prompt。
- [ ] close capability gating。
- [ ] permission 自动选择 reject。
- [ ] event 不含 raw tool input。

**验证命令**：同 T08。

**依赖任务**：T08。

### T10 实现 Aggregator 与 completion boundary

**目标**：聚合当前 session assistant text，拒绝 tool/permission，冻结最后 chunk 顺序。

**先读**：

- `04-acp-process-runtime-design.md` §5–6
- AionCore `protocol/events/translate.rs`

**涉及文件**：

- `src-tauri/src/backend/ai_execution/backends/mod.rs`
- `src-tauri/src/backend/ai_execution/backends/acp_aggregator.rs`
- `src-tauri/src/backend/ai_execution/mod.rs`

**优先编写测试**：EVT-01..12。

**验收标准**：

- [ ] wrong session 丢弃。
- [ ] Unicode chunk 顺序正确。
- [ ] cap exact 边界正确。
- [ ] permission/tool 返回 cancel action。
- [ ] completion 不依赖 sleep。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution::backends::acp_aggregator
```

**依赖任务**：T04、T09。

### T11 实现 Fake ACP 全链路与 AcpExecutionBackend

**目标**：把 Managed Process、Protocol、Aggregator 组合成一次 execution，并冻结 cleanup。

**先读**：

- `04-acp-process-runtime-design.md` §7–11
- `06-test-verification-acceptance.md` §6–9

**涉及文件**：

- `src-tauri/src/backend/ai_execution/backends/acp.rs`
- `src-tauri/src/backend/ai_execution/backends/mod.rs`
- `src-tauri/src/backend/ai_execution/backends/acp_fixture.rs`（或 colocated tests）
- `src-tauri/src/backend/agents/process.rs`
- `src-tauri/src/backend/agents/protocol/acp.rs`

**优先编写测试**：Fake modes + LIFE-01..11。

**验收标准**：

- [ ] session flow 顺序正确。
- [ ] app-owned temp workspace + empty MCP。
- [ ] 每个 `?` path 进入 cleanup。
- [ ] close error 后 kill/wait。
- [ ] result ready + cleanup fail 不返回 success。
- [ ] fake tests 不依赖网络/真实 OpenCode。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution::backends::acp -- --nocapture
```

**依赖任务**：T07、T09、T10。

### Checkpoint C：ACP Core

```bash
cargo fmt --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml backend::agents
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution
```

人工确认无 dialect shim、无完整 SessionBackend copy、无 raw payload log。

## 7. Phase 4：Executor 与 Application

### T12 实现 AgentExecutor 路由、并发与 total timeout

**目标**：Registry lookup、protocol route、shared semaphore、可取消 queue、deadline。

**先读**：

- `02-architecture-design.md` §7
- `01-product-requirements.md` FR-EXE

**涉及文件**：

- `src-tauri/src/backend/ai_execution/executor.rs`
- `src-tauri/src/backend/ai_execution/mod.rs`
- `src-tauri/src/backend/ai_execution/types.rs`
- `src-tauri/src/backend/agents/registry.rs`

**优先编写测试**：EXE-02..10。

**验收标准**：

- [ ] route 只 match protocol。
- [ ] max concurrency=2。
- [ ] queued cancel 不 spawn。
- [ ] timeout 触发 backend cancellation 并等待 cleanup。
- [ ] unknown/native error 稳定。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution::executor
```

**依赖任务**：T03、T04、T11。

### T13 注入 AgentExecutionRuntime 到 AppService/AppState

**目标**：建立可测试、Desktop 共享、Engine 可用的 runtime ownership。

**先读**：

- `05-translation-task-api-integration.md` §3–4
- 当前 application/service.rs、system.rs、adapters/app_state.rs

**涉及文件**：

- `src-tauri/src/backend/ai_execution/mod.rs`
- `src-tauri/src/backend/application/service.rs`
- `src-tauri/src/backend/application/system.rs`
- `src-tauri/src/adapters/app_state.rs`
- `src-tauri/src/lib.rs`

**优先编写测试**：injected fake runtime，AppService constructors。

**验收标准**：

- [ ] Tauri state 共享一个 runtime。
- [ ] Engine AppService 持有 runtime。
- [ ] tests 可注入 fake。
- [ ] OpenCode 未安装不阻止 app 启动。
- [ ] 现有 AppService 构造 tests 修复且无行为变化。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::application
cargo check --manifest-path src-tauri/Cargo.toml
```

**依赖任务**：T12。

### T14 迁移 OpenCode Translation，隔离 Gemini

**目标**：OpenCode 通过 AgentExecutor；Gemini 保持 legacy；DTO 不变。

**先读**：

- `05-translation-task-api-integration.md` §1–4
- 当前 `card_translation.rs`
- 当前 `application/card_translation.rs`

**涉及文件**：

- `src-tauri/src/backend/card_translation.rs`
- `src-tauri/src/backend/application/card_translation.rs`
- `src-tauri/src/backend/ai_execution/mod.rs`
- `src-tauri/src/backend/ai_execution/legacy_gemini.rs`

**优先编写测试**：TR-01..06、TR-09、TR-10、TR-12。

**验收标准**：

- [ ] OpenCode translation 构造 agent_id=opencode。
- [ ] OpenCode execution 无 `run` 参数。
- [ ] result 映射 `translated_text`。
- [ ] Gemini tests 保持。
- [ ] legacy seam 只含 Gemini，有删除注释。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::card_translation
```

**依赖任务**：T13。

### T15 迁移 Availability 与 Model Discovery

**目标**：OpenCode status/model list definition-driven，业务不拼 command。

**先读**：

- `05-translation-task-api-integration.md` §4.2–4.4、§13

**涉及文件**：

- `src-tauri/src/backend/agents/registry.rs`
- `src-tauri/src/backend/card_translation.rs`
- `src-tauri/src/backend/application/card_translation.rs`
- `src-tauri/src/backend/ai_execution/legacy_gemini.rs`

**优先编写测试**：REG-08..10、TR-07、TR-08。

**验收标准**：

- [ ] availability command 来自 Definition。
- [ ] model discovery command 来自 Definition。
- [ ] model list 去重、稳定、最多 500。
- [ ] Gemini manual behavior 不变。

**验证命令**：card translation + agents tests。

**依赖任务**：T14。

### Checkpoint D：Backend Feature

运行 Rust 全量定向，使用 fake runtime 手工走 AppService。评审公开 DTO 与 Gemini seam。

## 8. Phase 5：Desktop Background Tasks 与 Engine

### T16 扩展 BackgroundTaskRegistry

**目标**：存储 AI task snapshot/cancellation/phase/retention，并计入 running tasks。

**先读**：

- `05-translation-task-api-integration.md` §5
- 当前 `adapters/tauri/background_tasks.rs`

**涉及文件**：

- `src-tauri/src/adapters/tauri/background_tasks.rs`
- `src-tauri/src/backend/ai_execution/types.rs`

**优先编写测试**：TASK-01..15。

**验收标准**：

- [ ] full snapshot state machine。
- [ ] cancel 幂等。
- [ ] prompt 不入 snapshot。
- [ ] retention 正确。
- [ ] has_running_tasks 包含 AI。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml adapters::tauri::background_tasks
```

**依赖任务**：T12。

### T17 添加 Tauri Task Commands 与事件

**目标**：start/get/list/cancel，后台执行与 full snapshot event。

**先读**：

- `05-translation-task-api-integration.md` §5–6
- 现有 memory/backup task command 模式

**涉及文件**：

- `src-tauri/src/adapters/tauri/commands.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/adapters/app_state.rs`
- `src-tauri/src/adapters/tauri/background_tasks.rs`

**优先编写测试**：TAURI-01..06。

**验收标准**：

- [ ] start 快速返回。
- [ ] 不持 global lock。
- [ ] phase update emit full snapshot。
- [ ] terminal 在 cleanup 后 emit。
- [ ] cancel 不直接 kill。

**验证命令**：Tauri adapter tests + cargo check。

**依赖任务**：T13、T16。

### T18 保持 Engine Contract 并验证 CLI

**目标**：现有 Engine method 同步工作且共享 executor，contract 无破坏。

**先读**：

- `05-translation-task-api-integration.md` §7
- Engine registry translation entries

**涉及文件**：

- `src-tauri/src/adapters/engine/registry.rs`
- `cli/internal/schema/contract.json`（生成）
- 必要的 Engine integration test 文件

**优先编写测试**：API-01..08。

**验收标准**：

- [ ] canonical method/request/result 不变。
- [ ] OpenCode fake ACP execution 通过。
- [ ] generated contract 仅预期 diff。
- [ ] CLI interrupt cleanup 有证据。

**验证命令**：

```bash
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
```

**依赖任务**：T14。

### Checkpoint E：Adapters

Desktop task API 与 Engine sync API 都必须用同一个 AgentExecutor。评审确认没有第二套 execution flow。

## 9. Phase 6：Frontend

### T19 新增 Frontend Service 与 AiExecutionTaskProvider

**目标**：task invoke、event + polling provider、全局状态。

**先读**：

- `05-translation-task-api-integration.md` §8–9
- 现有 MemoryTaskProvider/SearchIndexProvider

**涉及文件**：

- `frontend/src/services/cardTranslation.ts`
- `frontend/src/services/cardTranslation.test.ts`
- `frontend/src/app/backgroundTasks/AiExecutionTaskProvider.tsx`
- `frontend/src/app/backgroundTasks/AiExecutionTaskProvider.test.tsx`
- `frontend/src/app/App.tsx`

**优先编写测试**：service/provider cases。

**验收标准**：

- [ ] start/get/list/cancel typed。
- [ ] full snapshot merge idempotent。
- [ ] missed event polling fallback。
- [ ] unmount cleanup listener/timer。
- [ ] Provider 在 app root。

**验证命令**：

```bash
pnpm test -- cardTranslation AiExecutionTaskProvider
pnpm typecheck
```

**依赖任务**：T17。

### T20 接入 ConversationContentCards

**目标**：block/task correlation、progress、cancel、success save、错误区分。

**先读**：

- `05-translation-task-api-integration.md` §10
- 当前 ConversationContentCards tests

**涉及文件**：

- `frontend/src/components/conversations/ConversationContentCards.tsx`
- 对应 test 文件
- `frontend/src/i18n/messages.ts`

**优先编写测试**：start/progress/unrelated controls/cancel/success/save failure/unmount。

**验收标准**：

- [ ] 当前 block 显示 phase。
- [ ] 其他操作仍可用。
- [ ] page unmount 不 cancel。
- [ ] AI success 与 persistence fail 分开。
- [ ] cancelled 不显示 failure banner。

**验证命令**：component test + typecheck。

**依赖任务**：T19。

### T21 全局任务提示与 App Close

**目标**：离开页面仍可见 running Agent task；关闭 app 能 cancel all 并等待 cleanup。

**先读**：

- `05-translation-task-api-integration.md` §11、§14
- 当前 AppClosePrompt 与其他 background indicator

**涉及文件**：

- 全局 task indicator component
- 对应 test
- `frontend/src/app/App.tsx`
- `frontend/src/app/AppClosePrompt.tsx`
- `src-tauri/src/lib.rs` 或 command helper（必要时）

**拆分规则**：若后端 close convergence 与 UI 超过 5 文件，拆为 T21A（backend close）和 T21B（frontend indicator）。

**优先编写测试**：TAURI-07/08、global indicator cases。

**验收标准**：

- [ ] running count/phase 可见。
- [ ] 可 cancel。
- [ ] app close 识别 AI task。
- [ ] confirm 后 5 秒内 cleanup convergence 或记录高优先级诊断。

**验证命令**：frontend tests + Rust close tests。

**依赖任务**：T17、T19。

### Checkpoint F：User Flow

```bash
pnpm typecheck
pnpm test
pnpm build
```

手工验证无关 UI 响应性。

## 10. Phase 7：全量验证与灰度

### T22 全量质量门与 Real OpenCode Smoke

**目标**：执行所有自动化与真实 OpenCode smoke，记录证据。

**先读**：

- `06-test-verification-acceptance.md` 全文

**涉及文件**：

- 本任务不应修改生产代码；失败回到对应 Task 修复。
- 可更新 `07-implementation-plan.md` 的验证记录。

**验证命令**：

```bash
cargo fmt --all -- --check
cargo test --workspace
pnpm typecheck
pnpm test
pnpm build
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

然后按 Real OpenCode smoke 12 步执行。

**验收标准**：

- [x] 所有命令 PASS。
- [x] smoke 证据完整。
- [x] cancel/app close 无残留 process。
- [x] log redaction 通过。
- [x] requirements traceability 无缺项。

**依赖任务**：T01–T21。

### T23 确认 Translation 唯一 OpenCode ACP 路径

**目标**：真实 smoke 后确认 Translation 仅通过 AgentExecutor/OpenCode ACP 执行；删除未使用的 Translation generic legacy seam，Gemini legacy seam 保留到 Native Spec。

**触发条件**：

- Real OpenCode smoke 在目标平台通过；
- 无阻塞兼容 bug；
- 用户确认结束灰度。

**涉及文件**：

- `src-tauri/src/backend/application/card_translation.rs`
- `src-tauri/src/adapters/engine/registry.rs`
- `cli/internal/schema/contract.json`（生成）
- 本 Spec 与进度文档

**范围说明**：Memory 是 `01-product-requirements.md` 已列出的非目标；按用户决定，本任务不迁移或适配 Memory 界面/AI 链路。

**验收标准**：

- [x] Translation 生产路径无 `opencode run` command/args 或 generic legacy executor 调用。
- [x] compatibility Engine method 保留原 method ID，但描述与 handler 明确指向 OpenCode ACP。
- [x] tests 不依赖 Translation old path。
- [x] OQ-006 标记 Resolved；Memory scope exception 有记录。

**依赖任务**：T22 + 产品确认。

## 11. 任务状态记录

执行时使用：

```text
[ ] pending
[~] in progress
[x] complete + verification evidence
[!] blocked + exact blocker
```

不要提前把后续任务标完成。每个 Checkpoint 由独立评审确认。

## 12. 预估风险顺序

优先验证高风险项：

1. ACP SDK 2.0 Rust API 与 toolchain。
2. SDK notification/prompt completion 顺序。
3. model selection wire compatibility。
4. process group/tree cleanup。
5. AppService async/blocking ownership。
6. Desktop task 与 Engine sync 双 adapter 共用 core。
7. frontend page unmount 后 task 恢复。

如果 T01/T08/T10 任一证据不成立，先更新 Spec 再继续，不在后续任务用 workaround 掩盖。
