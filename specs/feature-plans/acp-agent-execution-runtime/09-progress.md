# Progress: ACP Agent Execution Runtime

| 字段 | 值 |
|---|---|
| 总体状态 | Complete |
| 当前阶段 | Phase 7：Complete |
| 当前任务 | 无；T00–T23 全部完成 |
| 最后更新 | 2026-08-13 |
| 主仓库基线 | `f409803` |
| 实现提交 | 本次提交（`codex/acp-agent-runtime`） |
| 参考仓库基线 | AionCore `4bfd7e2cd6d6b3371e0b99525143cefc554c86` |

## 1. 总体进度

| Phase | 范围 | 状态 | 完成度 |
|---|---|---|---:|
| Phase 0 | 决策冻结 | Complete | 1 / 1 |
| Phase 1 | 类型、Registry 与依赖 | Complete | 4 / 4 |
| Phase 2 | 进程 Runtime | Complete | 3 / 3 |
| Phase 3 | ACP Protocol 与 Fake Agent | Complete | 4 / 4 |
| Phase 4 | Executor 与 Application | Complete | 4 / 4 |
| Phase 5 | Desktop Tasks 与 Engine | Complete | 3 / 3 |
| Phase 6 | Frontend | Complete | 3 / 3 |
| Phase 7 | 全量验证与灰度 | Complete | 2 / 2 |

任务总数：24（T00–T23）；已完成：24；进行中：0；待处理：0。

## 2. 已冻结决策

2026-08-13 按 Spec 推荐值冻结 Phase 1 默认决策；后续变化必须先更新 Spec。

| ID | 决策 | 状态 |
|---|---|---|
| D-001 | 每次 execution 启动独立 OpenCode ACP process，不建立 pool。 | Accepted |
| D-002 | Phase 1 Registry 使用代码内置定义，不新增 SQLite schema。 | Accepted |
| D-003 | OpenCode 使用 ACP；Gemini 保留明确的 legacy seam。 | Accepted |
| D-004 | 显式 model 设置失败即失败，不静默回退。 | Accepted |
| D-005 | Translation 严格 no-tool；permission 自动拒绝，tool activity 触发取消和失败。 | Accepted |
| D-006 | 使用 app-owned 空临时 workspace，不读取项目级源码、规则或配置。 | Accepted |
| D-007 | Desktop 使用后台 task snapshot；Engine 同步 command 保持兼容。 | Accepted |
| D-008 | ACP SDK 固定为兼容的 2.0.0。 | Accepted |
| D-009 | Phase 1 不实现 dialect shim。 | Accepted |
| OQ-006 | 不建立 OpenCode Translation 双路径 feature flag；T22 后直接确认 ACP 唯一路径。 | Resolved |

## 3. 任务状态

| Task | 内容 | 状态 | Commit | 验证 |
|---|---|---|---|---|
| T00 | 确认 Proposed 决策 | Complete | 本次提交 | 文档检查通过 |
| T01 | 验证并引入 ACP SDK | Complete | 本次提交 | `cargo check --manifest-path src-tauri/Cargo.toml` PASS |
| T02 | 定义 Agent 类型契约 | Complete | 本次提交 | 6 tests PASS |
| T03 | 实现 Builtin AgentRegistry | Complete | 本次提交 | 4 tests PASS |
| T04 | 重构 AiExecution 类型与错误 | Complete | 本次提交 | 10 tests + cargo check PASS |
| T05 | 提取 Host Process primitives | Complete | 本次提交 | 4 tests PASS |
| T06 | 实现 ManagedAgentProcess | Complete | 本次提交 | 10 core tests PASS |
| T07 | 加固 Process Tree 测试 | Complete | 本次提交 | 13 process tests PASS |
| T08 | 建立 ACP connection actor | Complete | 本次提交 | 4 actor tests PASS |
| T09 | 实现 Phase 1 ACP methods | Complete | 本次提交 | 4 operation/policy tests PASS |
| T10 | 实现 Aggregator 与 completion boundary | Complete | 本次提交 | EVT-01..12 PASS |
| T11 | Fake ACP 与 AcpExecutionBackend | Complete | 本次提交 | 9 stdio lifecycle tests PASS |
| T12 | AgentExecutor、并发与 timeout | Complete | 本次提交 | 8 executor tests PASS |
| T13 | Runtime 注入 AppService/AppState | Complete | 本次提交 | 89 application tests + cargo check PASS |
| T14 | 迁移 OpenCode Translation | Complete | 本次提交 | 12 translation/runtime tests + cargo check PASS |
| T15 | 迁移 Availability 与 Model Discovery | Complete | 本次提交 | 34 agent + 12 translation tests + cargo check PASS |
| T16 | 扩展 BackgroundTaskRegistry | Complete | 本次提交 | TASK-01..15 + 19 registry tests PASS |
| T17 | 添加 Tauri Task Commands | Complete | 本次提交 | TAURI-01..06 + 40 execution tests + cargo check PASS |
| T18 | 保持 Engine Contract 并验证 CLI | Complete | 本次提交 | 49 Engine tests + Go/CLI/e2e + interrupt smoke PASS |
| T19 | Frontend Service 与 Provider | Complete | 本次提交 | 8 service + 4 provider cases + typecheck PASS |
| T20 | 接入 ConversationContentCards | Complete | 本次提交 | 36 component cases + typecheck PASS |
| T21 | 全局任务提示与 App Close | Complete | 本次提交 | TAURI-07/08 + indicator/close tests + typecheck PASS |
| T22 | 全量质量门与 OpenCode Smoke | Complete | 本次提交 | 全量 gates + Real OpenCode 12-step hybrid smoke PASS |
| T23 | 确认 Translation 唯一 OpenCode ACP 路径 | Complete | 本次提交 | legacy audit + contract + full gates PASS |

## 4. 已完成工作

### 2026-08-13：需求与设计阶段

- 将原始需求整理为正式 Spec 文档集。
- 建立产品需求、架构、AionCore 代码映射、ACP/Process 详细设计、集成设计、测试矩阵、实施计划和 Lunna 执行手册。
- 更新 `~/fork-code/AionCore` 到参考 commit `4bfd7e2cd6d6b3371e0b99525143cefc554c86`。
- 校验文档内部链接、用户目录路径规则、需求编号覆盖和单任务文件数量。

验证证据：

```text
Spec files: 8 个子文档 + 1 个索引
Internal links: PASS
Absolute /Users path scan: PASS
Task file budget: T01–T21 均不超过 5 个列明文件
```

## 5. 当前执行记录

### T01：验证并引入 ACP SDK

状态：Complete

结论：

1. 固定 `agent-client-protocol = 2.0.0`，不启用 unstable feature。
2. SDK 最低 Rust 1.88.0；项目当前 Rust 1.96.0，兼容。
3. Tokio 增加 `io-util/macros/process/sync`；增加 `tokio-util/compat` 适配 SDK futures I/O。
4. AionCore 同样使用 ACP SDK 2.0.0，但其 fork/end-turn unstable features 不属于本阶段。

验证：

```text
cargo check --manifest-path src-tauri/Cargo.toml -> PASS
现有 warning 13 条，均为本任务前已有 dead_code warning；无新增编译错误。
```

### T02：定义 Agent 类型契约

状态：Complete

完成内容：

1. 新增 `AgentId`、`AgentProtocol`、`AgentDefinition`、`AgentCommandDefinition`、环境项和声明能力类型。
2. 固定 AgentId 为小写 ASCII 字母、数字、`-`、`_`，最大 64 bytes。
3. Definition validation 拒绝空 command/display name、NUL argument、非法 env key/value。
4. types 模块仅依赖 Rust 标准库，不依赖 Translation 或 ACP SDK。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::types
-> PASS（6 passed, 0 failed）
```

### T03：实现 Builtin AgentRegistry

状态：Complete

完成内容：

1. 实现代码内置 `AgentRegistry`，Phase 1 仅注册 OpenCode。
2. OpenCode launch definition 固定为 `opencode acp`，probe/discovery 分别为 `--version`/`models`。
3. lookup 返回 Registry 所有的不可变借用；构造时校验 definition 并拒绝 duplicate ID。
4. 通过 protocol-driven fixture 证明 ACP route 不依赖 Vendor id。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::registry
-> PASS（4 passed, 0 failed）
```

### T04：重构 AiExecution 类型与错误

状态：Complete

完成内容：

1. 将 `backend/ai_execution.rs` 迁移为 `backend/ai_execution/mod.rs` 目录模块，旧 helper 继续由原模块路径导出。
2. 新增统一 request/result/limits/purpose/phase/cancellation 类型，固定 Phase 1 limits。
3. 将内部错误移到 `error.rs`，新增稳定、安全、可序列化的 `AiExecutionErrorView`。
4. Request/Result Debug 隐去 prompt/text；公开错误 view 不包含进程路径、stdout 或 stderr。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution -> PASS（10 passed）
cargo fmt --all -- --check -> PASS
cargo check --manifest-path src-tauri/Cargo.toml -> PASS
```

### T05：提取可复用 Host Process primitives

状态：Complete

完成内容：

1. 将 process-group 配置提升为可复用 crate helper。
2. 新增跨平台 `signal_process_tree` 与 Terminate/Kill 信号语义。
3. Unix 将已退出的 process group 视为幂等成功；Windows 保留 `taskkill /T` 实现。
4. 原一次性 command runner 继续复用同一 helper，未改变 timeout/cancel 行为。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml backend::host_process
-> PASS（4 passed, 0 failed）
```

### T06：实现 ManagedAgentProcess

状态：Complete

完成内容：

1. 实现 Tokio 长生命周期 child、独立 process group、唯一 child wait owner 和 exit watch。
2. stdin/stdout 原子 single-take；stderr 持续 drain 到 bounded byte tail，并支持 lossy diagnostic。
3. 实现 graceful terminate、force kill tree、幂等 wait/report 和 Drop 最后防线。
4. Spawn preview 仅含 program、arg count、env key 与 cwd presence，不含 arg/env value。

### T07：加固 Process Tree 测试

状态：Complete

完成内容：

1. 增加 echo stdio、stderr burst/invalid UTF-8、immediate exit、env overlay 和重复 terminate 测试。
2. 增加 ignore-SIGTERM、direct child + grandchild、launcher 提前退出三类进程树测试。
3. 使用 readiness PID file，避免基于固定 sleep 的启动竞态。
4. Unix 验证记录的 process group 在 launcher 退出后仍可清理；Windows 保留 taskkill tree fixture 路径。

验证：

```text
cargo fmt --all -- --check -> PASS
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::process -- --nocapture
-> PASS（13 passed, 0 failed）
```

### T08：建立 ACP connection actor

状态：Complete

完成内容：

1. 使用 ACP SDK typed `Client` 与 `ByteStreams` 建立独立 connection actor，业务侧只持有 typed handle。
2. initialize 使用本地可配置 timeout，并缓存 `InitializeResponse`；失败和超时都会 abort/join actor。
3. Client identity 固定为 AssetIWeave，capabilities 保持最小值，不声明 filesystem、terminal 或 session 能力。
4. 提供 bounded event channel、disconnect watch、alive 状态和幂等 shutdown。

### T09：实现 Phase 1 ACP methods 与 permission handler

状态：Complete

完成内容：

1. 实现 typed `session/new`、`session/set_config_option`、`session/prompt`、`session/cancel`、`session/close`。
2. OpenCode model 选择固定使用 config id `model`；model request 有独立本地 timeout。
3. Prompt 只发送单个 `TextContent`；in-flight prompt 可接收 cancel notification。
4. Permission request 一律返回 `Cancelled`，事件只保留 session 与类别，不保留 raw tool input/output。
5. Close 严格按 initialize advertised capability gating。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::protocol::acp -- --nocapture
-> PASS（8 passed, 0 failed）
```

### T10：实现 Aggregator 与 completion boundary

状态：Complete

完成内容：

1. 实现仅聚合目标 session assistant text 的 `TranslationTextAggregator`。
2. Unicode 按完整 String chunk 拼接，输出上限按 UTF-8 byte 精确计算；超过上限前不写入部分 chunk。
3. Thinking/Other 不进入结果；permission/tool 返回 `CancelAndFail` 并使用稳定错误类型。
4. Protocol 在 prompt response 后向同一 bounded event channel 追加 `TurnCompleted` marker；此前 notification handler 已顺序完成入队。
5. Aggregator 仅在读到目标 session marker 后 finalize，不使用 sleep 或经验性 drain delay。
6. 统一 legacy/new output-limit 与 empty-output 错误形状，对外错误 view 保持安全。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution::backends::acp_aggregator -- --nocapture
-> PASS（12 passed, 0 failed）
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::protocol::acp -- --nocapture
-> PASS（8 passed, 0 failed）
```

### T11：实现 Fake ACP 与 AcpExecutionBackend

状态：Complete

完成内容：

1. 组合 `ManagedAgentProcess`、typed `AcpProtocol`、Aggregator 与隔离 workspace，形成单次 ACP execution backend。
2. 所有 spawn 后的业务错误均先形成 primary outcome，再进入统一 cleanup；没有用散落的 early-return cleanup。
3. Cleanup 固定执行 cancel（失败路径）、close、protocol shutdown、process tree terminate/kill、wait/reap、stderr join 与 workspace removal。
4. Close error/timeout 继续执行后续清理；业务结果已就绪但 cleanup 失败时返回 `CleanupFailed`，不返回 success。
5. 新增标准 NDJSON/JSON-RPC stdio fake ACP，覆盖 initialize/new/model/prompt/cancel/close、Unicode chunks、wrong session、thinking、late chunk、permission、tool、oversize、disconnect 与 exit。
6. Session workspace 是 backend 指定 app-owned root 下的 UUID 空目录；`session/new` 发送空 MCP 与空 additional directories。
7. Cancellation token 增加 `Notify` 驱动的异步等待，不用轮询或固定 sleep。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib 'backend::ai_execution::backends::acp::tests::' -- --nocapture
-> PASS（9 passed, 0 failed）
```

### T12：实现 AgentExecutor 路由、并发与 total timeout

状态：Complete

完成内容：

1. 新增 backend trait 与 `AgentExecutor`；Registry definition 只在执行边界 clone，路由只 match `AgentProtocol`。
2. 默认共享 `Semaphore(2)`；第三个 execution 保持 queued，前两个完成后才进入 backend。
3. Queue acquire 同时监听 cancellation 与总 deadline；queued cancel/timeout 从未调用 backend。
4. 获取 permit 后再次检查 cancellation，消除 permit/cancel 同时 ready 的竞态。
5. 总 deadline 包含 queue wait；backend 获得剩余预算而不是重新获得完整 timeout。
6. Executor deadline 触发后先 cancel token，再 await backend cleanup；`CleanupFailed` 优先于普通 timeout。
7. ACP backend 自身在 initialize/new/model/prompt 任一阶段监听 cancel/timeout，drop 阶段 future 后统一 cleanup。
8. Unknown agent 与 Native protocol 使用稳定错误；结果只暴露 `requested_model`，不声称 `model_used`。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib 'backend::ai_execution::executor::tests::' -- --test-threads=1
-> PASS（8 passed, 0 failed）
cargo test --manifest-path src-tauri/Cargo.toml --lib 'backend::ai_execution::backends::acp::tests::' -- --test-threads=1
-> PASS（9 passed, 0 failed）
```

### T13：注入 AgentExecutionRuntime 到 AppService/AppState

状态：Complete

完成内容：

1. 新增可注入的 `AgentExecutionRuntime` trait，`AgentExecutor` 提供生产实现。
2. `AppService` 持有 `Arc<dyn AgentExecutionRuntime>`，新增显式 runtime 注入构造器与访问器。
3. Tauri `AppState` 持有同一共享 runtime；默认 AppService 与 Engine 构造共享 executor/semaphore。
4. 默认 runtime 仅构建 builtin Registry/definition，不在 app startup 阶段 probe 或 spawn OpenCode。
5. 增加 injected fake runtime 与 shared runtime identity 测试，并修复所有直接构造 `AppService` 的测试 fixture。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::application
-> PASS（89 passed, 0 failed）
cargo check --manifest-path src-tauri/Cargo.toml
-> PASS
```

### T14：迁移 OpenCode Translation，隔离 Gemini

状态：Complete

完成内容：

1. OpenCode 翻译与兼容 DTO 统一映射为 `agent_id=opencode` 的 `AiExecutionRequest`，不再构造 `opencode run`。
2. Agent Runtime 返回的 text 保持映射到现有 `translated_text`，公开 Translation DTO 未改变。
3. Connection test 同样走 Agent Runtime，使用 `ConnectionTest` purpose 和 30 秒 total timeout。
4. 新增专用 synchronous/Engine blocking facade：在独立线程驱动 async runtime，避免已有 Tokio runtime 中嵌套 `block_on`。
5. Gemini 移入带明确删除条件的 `legacy_gemini` seam；新 Agent 不能加入该分支。
6. Tauri compatibility commands 通过 `AppState.agent_runtime` 构造 AppService，Engine 继续使用同一 application method。
7. 业务 prompt 200k 限制、model validation 与 Google/Apple reserved error 保持不变。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::card_translation
-> PASS（10 passed, 0 failed）
cargo test --manifest-path src-tauri/Cargo.toml --lib 'backend::application::card_translation::tests::'
-> PASS（1 passed, 0 failed）
cargo test --manifest-path src-tauri/Cargo.toml --lib 'backend::ai_execution::legacy_gemini::tests::'
-> PASS（1 passed, 0 failed）
cargo check --manifest-path src-tauri/Cargo.toml
-> PASS
```

### T15：迁移 Availability 与 Model Discovery

状态：Complete

完成内容：

1. `AgentRegistry` 新增 availability/model discovery 通用 probe，command 与 args 只来自 `AgentDefinition`。
2. Probe 复用 host process 的 timeout、bounded stdout/stderr 和 process-tree cleanup，分类 not-found、timeout、spawn/output、output-limit 与 non-zero exit。
3. 可执行文件解析下沉为通用 host helper，继续支持 PATH、login shell 和常见安装目录；ACP execution spawn 也每次重新解析。
4. `AgentExecutionRuntime` 暴露可注入的 availability/discovery seam，生产 `AgentExecutor` 使用其共享 Registry。
5. OpenCode status 和 model list 业务不再拼接 `--version`/`models`；model list 稳定排序、去重并限制 500 条。
6. Gemini model list 仍保持 manual-input 提示，未加入 Registry。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::agents
-> PASS（34 passed, 0 failed）
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::card_translation
-> PASS（12 passed, 0 failed）
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::application::card_translation
-> PASS（1 passed, 0 failed）
cargo check --manifest-path src-tauri/Cargo.toml
-> PASS
```

### T16：扩展 BackgroundTaskRegistry

状态：Complete

完成内容：

1. 在现有中央 `BackgroundTaskRegistry` 增加 AI execution entries，没有新建第二个全局 registry。
2. 实现 queued/running/completed/failed/cancelled 状态机、execution phase、完整时间戳、公开 result 与稳定 error view。
3. 实现 begin/update/finish/cancel/get/list/cancel-all API；cancel 只设置 token 并进入 Cancelling，terminal cancel 幂等。
4. Snapshot 类型不包含 prompt、workspace/cwd、environment 或 stderr 字段。
5. Terminal task 保留 10 分钟、最多 100 条；queued/running 永不被 retention 淘汰。
6. `has_running_tasks` 已计入 AI tasks；poisoned lock 对 API 返回 error，running check fail-closed。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib adapters::tauri::background_tasks
-> PASS（19 passed, 0 failed，覆盖 TASK-01..15）
```

### T17：添加 Tauri Task Commands 与事件

状态：Complete

完成内容：

1. 新增 `start/get/list/cancel` 四个 Tauri AI execution task commands，并注册到 command handler。
2. Start 仅做业务校验和 queued snapshot 创建，然后立即返回；执行在 Tauri async runtime 后台运行，不获取全局 app lock。
3. 新增 `AiExecutionProgressSink`，Executor/ACP backend 报告 resolving、spawning、initializing、creating-session、configuring、prompting、cancelling、closing 与 cleaning-up。
4. 每次 phase/state 变化使用 `ai-execution://task-updated` 发送完整 snapshot，不发送 token/chunk。
5. Terminal snapshot 只在 runtime future 返回（即 executor cleanup 完成）之后发送；panic/join failure 收敛为稳定 protocol error。
6. Cancel command 只设置 Registry token 并发送 Cancelling snapshot，不在 command 线程 kill 进程。
7. 新 task API 只承载 OpenCode Agent translation；Gemini 继续使用现有 compatibility API/legacy seam。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib 'adapters::tauri::commands::tests::tauri_'
-> PASS（3 passed, 0 failed，覆盖 TAURI-01..06）
cargo test --manifest-path src-tauri/Cargo.toml --lib adapters::tauri::background_tasks
-> PASS（19 passed, 0 failed）
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::ai_execution -- --test-threads=1
-> PASS（40 passed, 0 failed）
cargo check --manifest-path src-tauri/Cargo.toml
-> PASS
```

### T18：保持 Engine Contract 并验证 CLI

状态：Complete

完成内容：

1. 新增 canonical Engine method `conversation.card.translation.run`，继续绑定既有 `ConversationTranslationRequest` 与 `translated_text` 结果，不建立第二套执行流程。
2. 重新生成 CLI contract；语义 diff 仅包含该 canonical method，兼容 Tauri method 继续保留。
3. Engine stdio 使用与 AppService 相同的共享 `AgentExecutor`；Fake OpenCode ACP 从 canonical method 完成 initialize/new/model/prompt/close 全链路。
4. `AgentExecutor` 跟踪 active cancellation token，并提供进程级 `cancel_all`；Engine 通过 async-signal-safe flag 接收 SIGINT/SIGTERM，再在普通线程触发取消。
5. Go CLI root 使用 signal-aware context；Engine client 先向子 Engine 转发 interrupt 并给出 5 秒 cleanup 窗口，随后才由 `CommandContext` 强制结束。
6. Tauri task/app-close commands 明确列入 desktop-only contract drift allowlist；Engine 继续暴露同步 canonical API。
7. 实际 interrupt smoke 在 0.045 秒内收敛，Fake ACP 记录 cancel/close/stdin_closed，workspace 已删除，无 execution 目录残留。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml adapters::engine -- --nocapture
-> PASS（49 passed, 0 failed）
cargo check --manifest-path src-tauri/Cargo.toml
-> PASS
go vet -C cli ./...
-> PASS
go test -C cli -race ./...
-> PASS
pnpm cli:test:e2e
-> PASS
真实 Engine + Fake ACP canonical execution -> PASS
SIGINT cleanup smoke -> PASS（cancel + close + stdin_closed；无 workspace 残留）
```

### T19：新增 Frontend Service 与 AiExecutionTaskProvider

状态：Complete

完成内容：

1. `cardTranslation` service 新增完整的 task state/phase/error/snapshot 类型和 start/get/list/cancel typed invoke；start 复用现有安全 prompt renderer。
2. 新增 app-root `AiExecutionTaskProvider`，启动时恢复 task list，并通过 full-snapshot Tauri event 增量合并。
3. queued/running task 启用 1 秒 polling fallback；provider unmount 会清理 listener 与 timer。
4. snapshot 按 id 幂等合并，并按 `updated_at` 与状态/阶段进度拒绝 stale event，避免 polling 覆盖更新的 terminal snapshot。
5. Context 暴露 tasks/start/cancel/get/refresh；只限制冲突动作，不提供 page-level busy gate。
6. Provider 按现有 background provider 结构挂载到 `AppProviders`，覆盖 Router 与 AppClosePrompt 的全局生命周期。

验证：

```text
pnpm test -- cardTranslation
-> PASS（78 files / 437 tests；cardTranslation 8 cases）
pnpm test -- AiExecutionTaskProvider
-> PASS（AiExecutionTaskProvider 4 cases）
pnpm typecheck
-> PASS
```

### T20：接入 ConversationContentCards

状态：Complete

完成内容：

1. 生产默认路径改为 `AiExecutionTaskProvider.startTranslation`；同步 translator 仅保留显式注入的 compatibility test seam。
2. 新增 block -> task id correlation；每个 block 独立显示 queued/start/connect/model/prompt/cancel/cleanup phase，不使用页面级 busy。
3. 当前 block 在 active task 期间只禁用自身 translate；复制、其他 block 和页面操作保持可用。
4. active task 提供键盘可达的 cancel action；cancelling/cleaning-up 阶段防止重复 cancel。
5. succeeded result 先进入 translated UI，再独立持久化 part translation；保存失败使用独立错误文案且保留成功的 AI result。
6. failed task 显示稳定 error message；cancelled 作为中性 terminal state，不展示 failure banner。
7. task terminal 只处理一次；page unmount 不调用 cancel，后台 task 继续由全局 Provider 跟踪。
8. 新增中英文 phase、cancel、save-failure 和 missing-result 文案。

验证：

```text
pnpm test -- ConversationContentCards
-> PASS（78 files / 442 tests；ConversationContentCards 36 cases）
pnpm typecheck
-> PASS
```

### T21：全局任务提示与 App Close

状态：Complete

完成内容：

1. 新增 app-root Agent task indicator，显示 queued/running 总数和最近 task phase，并可直接取消最近 active task。
2. indicator 使用既有 cockpit task surface、semantic theme tokens、`aria-live` 与原生 button，不阻塞页面交互。
3. `BackgroundTaskRegistry` 新增 cancel-all + bounded convergence wait；只在无 active AI execution 后报告 converged。
4. Desktop Window Close、App Exit 与 frontend `complete_app_close` 三条确认路径统一先取消 AI tasks，最多等待 5 秒 cleanup，再继续原关闭流程。
5. cleanup 超时或 registry error 写入 `app.close.ai_execution` high-priority operation diagnostic，并保留 pending/cancelled count。
6. TAURI-07/08 覆盖 running detection、全量 token cancellation、cleanup convergence 与 bounded timeout；不使用固定无界等待。
7. 新增中英文全局 task count、phase 与 cancel 文案。

验证：

```text
cargo test --manifest-path src-tauri/Cargo.toml app_close -- --nocapture
-> PASS（TAURI-07/08，2 passed）
cargo check --manifest-path src-tauri/Cargo.toml
-> PASS
pnpm test -- AiExecutionTaskIndicator AppClosePrompt
-> PASS（79 files / 444 tests）
pnpm typecheck
-> PASS
```

### T22：全量质量门与 Real OpenCode Smoke

状态：Complete

完成内容：

1. 全量 Rust、Frontend、Engine contract、Go CLI 与 CLI-to-Engine e2e 质量门全部通过；同步修正 3 个已落后于当前 Codex adapter 行为的 Rust 断言，未修改 adapter 生产逻辑。
2. 本机真实 OpenCode `1.18.12` availability 检查通过；model discovery 返回 176 个排序去重 model，列表摘要为 `04604c8ab3bfffdd`。
3. 不指定 model 与指定有效 model 的 canonical Engine Translation 均通过；只记录输出字节数和摘要，不保存 prompt/result 内容。
4. 指定无效 model 返回 `operation_error` 且无成功结果，证明未静默回退。
5. 对真实 `opencode acp` 进程执行 Engine `SIGINT`：0.039 秒内完成取消、关闭、进程树回收和 Engine 收敛，无强制 kill、无残留 ACP process。
6. `pnpm tauri:dev` 使用临时数据库成功启动真实桌面进程与 Vite；终止后 desktop/Vite/OpenCode ACP process 均无残留。
7. 真实 execution 日志只包含 `execution_id`、agent/protocol/phase、pid、计数、耗时和 cleanup 状态；prompt marker、result、环境变量 secret 均未出现。
8. 为满足 NFR-006，补齐安全的 execution lifecycle/phase/process/output/cleanup metadata；日志不写 prompt、model、result、raw stderr、完整 args/env 或 ACP payload。
9. Requirements Traceability Matrix 已补齐 `FR-REG-005` 与 `NFR-001..006` 的验证映射。

Real OpenCode 12-step evidence（真实进程与自动化组合）：

| Step | 结果 | 证据 |
|---:|---|---|
| 1 | PASS | `pnpm tauri:dev` 启动 desktop + Vite；临时 DB/log 目录 |
| 2 | PASS | 真实 availability `available=true`、version=`1.18.12`；Settings service 映射测试覆盖 |
| 3 | PASS | 真实 model count=176、sorted/unique、摘要 `04604c8ab3bfffdd` |
| 4 | PASS | default model success；result=46 bytes，摘要 `3f5dd7190ac06503` |
| 5 | PASS | 有效 model（ID 仅记录摘要 `f17f33acd6fa`）success；result=21 bytes，摘要 `303aa09590d45f29` |
| 6 | PASS | invalid model `ok=false`、`operation_error`、无 result |
| 7 | PASS | 真实 ACP child observed；cancel cleanup=0.039 s；remaining=0 |
| 8 | PASS | `EXE-05` 三任务验证最大并发=2，第三个保持 queued |
| 9 | PASS | Provider 恢复/event/polling + global indicator/component unmount tests |
| 10 | PASS | TAURI-07/08 cancel-all + bounded convergence；真实 Engine interrupt cleanup |
| 11 | PASS | Tauri/Engine smoke 后 `opencode acp`、desktop、Vite 均无新增残留 |
| 12 | PASS | 13 行/3033 bytes safe metadata；prompt/env/result 均未命中日志 |

验证：

```text
cargo fmt --all -- --check
-> PASS
cargo test --workspace
-> PASS（551 passed；1 个既有 release-only performance fixture ignored）
pnpm typecheck
-> PASS
pnpm test
-> PASS（79 files / 446 tests）
pnpm build
-> PASS
pnpm cli:contract
-> PASS
go vet -C cli ./...
-> PASS
go test -C cli -race ./...
-> PASS
pnpm cli:test:e2e
-> PASS
真实 OpenCode availability/model/default/valid/invalid/cancel/log-redaction smoke
-> PASS
```

说明：Smoke 记录不包含真实 prompt、result 或认证信息；临时响应与日志均位于系统临时目录，未加入仓库。

### T23：确认 Translation 唯一 OpenCode ACP 路径

状态：Complete

完成内容：

1. Translation 生产目录审计确认无 `AiCliRuntime`、`execute_structured_text`、`opencode run` 参数构造或 generic legacy executor 调用。
2. 删除 `AppService::execute_ai_structured_text` 这一未使用的 Translation generic legacy seam；OpenCode compatibility method 保留 method ID/DTO，但 handler 始终进入共享 `AgentExecutor`。
3. 修正 compatibility Engine contract 描述，明确 prompt 交给 OpenCode ACP Agent；重新生成 contract，语义变更仅为 canonical method 和该描述。
4. OQ-006 已关闭：实现中从未建立 OpenCode Translation 双执行路径或 feature flag，T22 真实 smoke 后直接确认 ACP 为唯一 Translation 路径。
5. 按用户最新决定，不迁移、不适配 Memory 界面和既有 Memory AI 链路；`AiCliRuntime::Opencode` 仅作为该明确非目标的 legacy 例外保留，等待 Memory 重写 Spec。
6. availability Tauri command 移入 `spawn_blocking`，避免外部 CLI probe 阻塞 async command handler。
7. frontend provider 对 terminal task 保留数量设为 100，始终保留全部 active task，防止长时间桌面会话累积无界历史。
8. 全局 cancel indicator 捕获并展示取消请求失败，避免 rejected promise 漏出。
9. 完成 correctness、readability、architecture、security、performance 五轴审查；未发现 blocking finding。

验证：

```text
Translation legacy audit
-> PASS（0 matches）
Memory legacy exception audit
-> PASS（仅 memory_extraction.rs、memory_dream.rs 与通用 legacy seam）
pnpm cli:contract
-> PASS
cargo fmt --all -- --check
-> PASS
cargo test --workspace
-> PASS（551 passed；1 ignored）
pnpm typecheck
-> PASS
pnpm test
-> PASS（79 files / 446 tests）
pnpm build
-> PASS
go vet -C cli ./...
-> PASS
go test -C cli -race ./...
-> PASS
pnpm cli:test:e2e
-> PASS
git diff --check
-> PASS
```

## 6. 当前风险与观察

| ID | 观察 | 影响 | 处理 |
|---|---|---|---|
| R-001 | 当前工作区存在用户正在进行的 App Close / Settings 未提交修改。 | 后续 T13/T17/T21 会触及相同文件。 | Phase 1–4 先避免这些文件；进入相关任务前重新检查 diff，不覆盖用户修改。 |
| R-002 | 显式 `TurnCompleted` marker 已通过 in-process 与真实 stdio late-chunk fixture。 | 风险已收敛。 | 保留 EVT-12 与 stdio regression test。 |
| R-003 | OpenCode ACP 2.0 model wire 已确认使用 `session/set_config_option`，config id 为 `model`。 | 风险已收敛。 | T09 已用 typed in-process agent 测试固定 wire。 |
| R-004 | `AppService` 仍会每次打开数据库，但 Agent runtime 已在进程内共享。 | runtime/semaphore ownership 风险已收敛。 | T13 已提供共享生产 runtime 与 fake injection seam。 |
| R-005 | 用户计划删除并重写全部 Memory 界面与流程。 | 本 Spec 若顺带迁移 Memory 会制造即将废弃的适配层。 | 本轮明确不修改 Memory UI/AI 链路；仅保证 ACP/Engine/CLI/Translation 消息连通，Memory 重写另立 Spec。 |

## 7. 阻塞项

当前无阻塞项。

## 8. 下一步

1. 按独立 Spec 删除并重写 Memory 界面与相关执行链路；本 Spec 不为旧 Memory UI 增加适配。
2. 如需引入 Gemini/其他 Agent，基于 Registry + protocol route 新增 backend，不进入 legacy seam。
3. 在目标 Windows/Linux 发布环境执行平台 smoke；Phase 1 当前真实 smoke 证据来自 macOS arm64。

## 9. 更新规则

每个任务结束时必须更新：

- 表头的当前阶段、当前任务和最后更新时间；
- Phase 完成度；
- 任务状态、commit 与验证摘要；
- “已完成工作”中的变更和证据；
- 新风险、Spec 偏差和阻塞项；
- 下一步。

只有验证命令通过并满足该任务全部 Acceptance 时，状态才能从 In Progress 改为 Complete。
