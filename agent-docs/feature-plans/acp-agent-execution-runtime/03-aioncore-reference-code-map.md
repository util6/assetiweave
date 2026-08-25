# 参考代码映射：AionCore / OpenCode (Reference Code Map)

| 字段 | 值 |
|---|---|
| 状态 | Verified Reference Map |
| AionCore 基线 | `4bfd7e2cd6d6b6b3371e0b99525143cefc554c86` |
| OpenCode 检视基线 | `8721a2710b37290041b17476e8c1f8bfcd49cbc9` |
| 参考目录 | `~/fork-code/AionCore`、`~/fork-code/opencode` |

> 行号只对上述 commit 有效。参考仓库更新后，先重新定位 symbol，再更新本文。实现者不得把旧行号当成 API 稳定承诺。

## 1. 使用方法

每项参考分为：

- **观察**：目标项目实际如何实现。
- **借鉴**：AssetIWeave 应采用的设计思想或最小代码形状。
- **不借**：Phase 1 不应复制的复杂度。
- **落点**：AssetIWeave 目标模块。

执行者应优先阅读指定 symbol 附近代码，不要把整个 AionCore crate 加入上下文。

## 2. 总映射

| AssetIWeave 目标 | AionCore / OpenCode 参考 | 主要 symbol |
|---|---|---|
| `AgentDefinition` / Registry | AionCore registry + DB seed | `AgentRegistry`, `decode_row`, OpenCode seed row |
| protocol route | AionCore ACP factory | `BackendRoute`, `route_for_backend`, `build` |
| Managed process | AionCore CLI process | `CliAgentProcess`, `spawn_for_sdk`, `take_stdio`, `kill` |
| ACP connection | AionCore protocol | `AcpProtocol::connect`, `run_sdk_background` |
| ACP operations | AionCore protocol | `new_session`, `prompt`, `cancel`, `close_session`, `set_model` |
| Session new flow | AionCore manager | `open_session_new` |
| Event normalization | AionCore events | `session_notification_to_events` |
| Permission routing | AionCore protocol/permission | `handle_permission_request` |
| Dialect boundary | AionCore dialect | `classify_incoming_line` |
| Transport seam | AionCore new session crate | `SessionBackend`, `BackendConnection` |
| OpenCode ACP capabilities | OpenCode ACP service | `initialize`, `newSession`, `closeSession`, `setSessionModel` |
| OpenCode stdio entry | OpenCode CLI | `AcpCommand` |

## 3. Agent Registry

### 3.1 AionCore 代码

文件：

```text
~/fork-code/AionCore/crates/aionui-ai-agent/src/registry.rs
```

重点：

- `AgentRegistry`：约 102–135 行。
- lookup `get`：约 443–445 行。
- visible list：约 468–508 行。
- row -> metadata projection `decode_row`：约 769–851 行。
- availability probe 与 unavailable reason：搜索 `probe_with_reason`、`probe_resolved_command`。
- handshake 回写：搜索 `apply_handshake_inner`、`CatalogSyncMessage`。

AionCore 的核心结构：

```rust
pub struct AgentRegistry {
  repo: Arc<dyn IAgentMetadataRepository>,
  by_id: RwLock<HashMap<String, AgentMetadata>>,
  unavailable_reasons: RwLock<HashMap<String, UnavailableReason>>,
  catalog_tx: mpsc::Sender<CatalogSyncMessage>,
  probe_policy: ProbePolicy,
}
```

### 3.2 观察

1. 定义先从 repository 投影到统一 metadata，再供 factory 使用。
2. `command`、`args`、`env`、capabilities、models、modes 都在 catalog，而不是业务代码。
3. availability 与 unavailable reason 被分开建模。
4. handshake observation 可回写 catalog。
5. runtime spawn 和 availability 使用同一投影结果，避免两套定义。

### 3.3 AssetIWeave 借鉴

- 使用 `HashMap<AgentId, AgentDefinition>` 作为唯一定义来源。
- definition 包含 command/args/env/protocol/probe。
- availability 返回 enum reason，不返回不可解析字符串。
- executor 再次解析 command，避免 stale cache。
- handshake observation 进入内存 map。

### 3.4 Phase 1 不借

- 不引入 `IAgentMetadataRepository`。
- 不引入 Agent metadata SQLite schema。
- 不引入 per-user override、remote rows、sort order、icon、i18n。
- 不引入 catalog sync consumer 与 DB write serialization。
- 不复制 AionCore 的完整 probe history。

### 3.5 AssetIWeave 落点

```text
src-tauri/src/backend/agents/types.rs
src-tauri/src/backend/agents/registry.rs
```

## 4. OpenCode 内置定义

### 4.1 AionCore 代码

文件：

```text
~/fork-code/AionCore/crates/aionui-db/migrations/001_initial_schema.sql
```

约 264–270 行：

```text
backend = opencode
agent_type = acp
command = opencode
args = ["acp"]
native_skills_dirs = [".opencode/skills"]
```

### 4.2 借鉴

Phase 1 definition 固定为：

```text
id       = opencode
protocol = acp
command  = opencode
args     = ["acp"]
```

### 4.3 不借

Translation 不注入 `.opencode/skills`，不采用 AionCore 的 `yolo_id = build`，也不复制 Team/side-question policy。

理由：Translation 是 no-tool、空 workspace 的一次性任务，而 AionCore row 面向完整 Agent Chat。

## 5. Runtime 路由

### 5.1 AionCore 代码

文件：

```text
~/fork-code/AionCore/crates/aionui-ai-agent/src/factory/acp.rs
```

重点：

- `BackendRoute`：约 25–39 行。
- `route_for_backend`：约 41–47 行。
- `build`：约 49 行开始。
- catalog metadata resolution：搜索 `resolve_catalog_metadata`。

### 5.2 观察

AionCore 当前存在三种 route：

```rust
BackendRoute::DirectCli
BackendRoute::Antigravity
BackendRoute::AcpManager
```

它证明“Agent family”与“实际 transport”不能混为一谈。

### 5.3 AssetIWeave 借鉴

- 业务请求先通过 Registry 变成 Definition。
- Executor 按 `AgentProtocol` 选择 backend。
- Registry definition 比 caller 传入的 Vendor hint 更可信。

### 5.4 不照搬

AionCore 的 `route_for_backend` 仍按 backend label 分支，是其历史 strangler 需要。AssetIWeave 新 Runtime 应直接把 route 写入 `AgentDefinition.protocol`，不要复制 Vendor label match。

目标：

```rust
match definition.protocol {
  AgentProtocol::Acp => ...,
  AgentProtocol::Native => ...,
}
```

## 6. CLI Process 与 SDK stdio ownership

### 6.1 AionCore 代码

文件：

```text
~/fork-code/AionCore/crates/aionui-ai-agent/src/capability/cli_process/mod.rs
~/fork-code/AionCore/crates/aionui-ai-agent/src/capability/cli_process/spawn_sdk.rs
```

重点 symbol：

- `CliAgentProcess`：`mod.rs` 文件前部。
- `take_stdio`：搜索该 symbol。
- `kill`：搜索 `pub async fn kill`。
- `force_kill_tree`：约 146–163 行。
- `wait_for_exit`：约 188–199 行。
- `take_stderr` / `peek_stderr_tail`：约 201–244 行。
- `spawn_for_sdk`：`spawn_sdk.rs` 约 15–112 行。
- stdio 只能 take 一次的测试：`spawn_sdk.rs` 约 247–259 行。
- spawn preview 不泄漏 args/env value 的测试：约 218–245 行。

### 6.2 观察

1. SDK mode 不启动 stdout reader，stdout 交给 ACP SDK。
2. stderr 独立后台 drain 到有界 tail buffer。
3. child exit 通过 watch channel 发布。
4. process group id 在 spawn 时记录。
5. `force_kill_tree` 即使 direct child 已退出也会 signal group，避免 launcher 孙进程泄漏。
6. spawn log 只显示 program、arg count、env key names、cwd，不打印 secret value。

### 6.3 AssetIWeave 借鉴

- `ManagedAgentProcess::take_stdio()` 只能成功一次。
- stderr drain 和 exit monitor 与 protocol 并行。
- cleanup 要有 graceful close 与 force kill 两级。
- 记录 process group，不只保存 pid。
- safe spawn preview 不打印 arg body/env value。

### 6.4 不照搬

- 不复制 AionCore `aionui-runtime::Builder` 与完整 agent env 构建器。
- 不引入其 process registry/store。
- 不复制 Chat runtime 的 idle/suspend 管理。

AssetIWeave 应扩展现有 `backend/host_process.rs` 的平台 primitives，保持单 Rust package。

## 7. ACP Protocol connection

### 7.1 AionCore 代码

文件：

```text
~/fork-code/AionCore/crates/aionui-ai-agent/src/protocol/acp.rs
```

重点：

- client info / initialize request：约 71–110 行。
- `AcpProtocol`：约 137 行开始。
- `connect`：约 168–249 行。
- `initialize_response` / capabilities：约 253–270 行。
- `new_session`：约 277 行。
- `close_session`：约 321 行。
- `prompt`：约 329 行。
- `cancel`：约 333 行。
- `set_model`：约 363 行。
- `run_sdk_background`：约 603 行开始。
- notification handler：`run_sdk_background` 中搜索 `on_receive_notification`。
- permission request handler：搜索 `handle_permission_request`。
- log redaction tests：搜索 `log_client_request_omits_prompt_body`。

### 7.2 观察

1. `connect` 启动后台 SDK actor，以 oneshot 返回 initialize 结果和 connection handle。
2. `prompt` future 与 connection actor 分离，所以 cancel notification 可并发发送。
3. initialize response 被缓存，供 capability gating。
4. session/update 经 channel fan-out。
5. permission request 经专门 responder channel 返回选择。
6. prompt body 不进入普通日志。
7. AionCore 为 config RPC 和 initialize 设置局部 timeout。

### 7.3 AssetIWeave 借鉴

- 同一 connection actor 驱动 initialize/prompt/cancel。
- protocol handle 持有 connection 与 shutdown signal。
- typed wrapper 只暴露 Phase 1 methods。
- notification 输出最小内部 event。
- permission handler在 Translation policy 下立即 reject/cancel。
- prompt 与 result 内容全程日志脱敏。

### 7.4 不照搬

- 不实现 terminal request handlers。
- 不实现 auth UI、list/load/resume/fork/ext method。
- 不实现 replay suppression。
- 不实现完整 broadcast `AgentStreamEvent`。
- 不实现 multi-session connection。

## 8. Session creation 与 reconcile

### 8.1 AionCore 代码

文件：

```text
~/fork-code/AionCore/crates/aionui-ai-agent/src/manager/acp/agent_session_flow.rs
```

重点：

- `open_session_new`：约 35–81 行。
- `session/new` response 中读取 session id、modes、config options。
- `reconcile_session`：搜索 symbol，观察 model/mode/config 的统一应用。

### 8.2 观察

AionCore flow：

```text
new_session_request
  -> protocol.new_session
  -> capture session_id
  -> apply advertised models/modes/config
  -> save state
  -> reconcile desired config
```

### 8.3 AssetIWeave 借鉴

Translation 简化为：

```text
new_session(cwd=temp, mcp=[])
  -> capture session_id
  -> if model specified: set model
  -> prompt
```

必须在 prompt 前完成 model。Session id 只存在 execution guard。

### 8.4 不照搬

- 不 commit DB state。
- 不 emit SessionAssigned。
- 不 resume/rebuild stale session。
- 不处理 preset/skill prelude。
- 不实现 mode/config reconcile 框架。

## 9. Event normalization

### 9.1 AionCore 代码

文件：

```text
~/fork-code/AionCore/crates/aionui-ai-agent/src/protocol/events/mod.rs
~/fork-code/AionCore/crates/aionui-ai-agent/src/protocol/events/translate.rs
```

重点：

- `AgentStreamEvent`：`mod.rs` 前部。
- `session_notification_to_events`：`translate.rs` 约 21 行。
- `SessionUpdate::AgentMessageChunk`：约 26 行。
- `AgentThoughtChunk`：约 34 行。
- `ToolCall`：约 47 行。
- `ToolCallUpdate`：约 65 行。
- `permission_request_to_event_data`：约 145 行。

### 9.2 观察

AionCore 没有让 UI 消费 ACP 原始 JSON，而是在 protocol boundary 归一化：

- assistant message -> text event；
- thought -> thinking event；
- tool call -> structured tool event；
- permission -> structured permission event。

### 9.3 AssetIWeave 借鉴

Phase 1 只建立内部 aggregator event：

- assistant text -> append；
- thought -> ignore/debug count；
- tool -> policy violation；
- permission -> deny；
- wrong session -> ignore + diagnostic。

### 9.4 不照搬

不复制完整 Tool Card、Plan、Skill Suggest、Cron、Usage、AvailableCommands UI 模型。

## 10. ACP Dialect shim

### 10.1 AionCore 代码

文件：

```text
~/fork-code/AionCore/crates/aionui-ai-agent/src/protocol/acp_dialect.rs
```

重点：

- `LineDisposition`；
- `classify_incoming_line`；
- CodeBuddy `session_end` / `compact-maxtoken` 识别；
- 其他行原样 Forward 的测试。

### 10.2 借鉴原则

兼容层只能位于 wire transport 输入边界：

```text
raw line -> known dialect classifier -> standard SDK
```

它只能吸收/转换已知非标准 shape，其他输入必须原样交给 SDK。

### 10.3 Phase 1 结论

OpenCode 当前走标准 ACP SDK，不创建 dialect module。只有提供以下证据后才新增：

1. 可复现 raw frame；
2. SDK 失败现象；
3. Agent/version 范围；
4. 最小修正与回归测试。

## 11. SessionBackend abstraction

### 11.1 AionCore 代码

文件：

```text
~/fork-code/AionCore/crates/aionui-session/src/backend/mod.rs
```

重点：

- `SessionBackend`：约 65–130 行。
- `BackendConnection`：约 132–151 行。

接口核心：

```text
SessionBackend: dispatch / events / capabilities / terminate
BackendConnection: open_session / close_session / capabilities
```

### 11.2 借鉴

只借“transport sealed behind backend”：

```text
Business -> AgentExecutor -> Backend -> Transport
```

### 11.3 不照搬

Translation 没有多轮 session actor、command mailbox、state reducer、rehydrate、capability-driven UI，因此 Phase 1 不定义完整 `SessionBackend`。

如果未来进入 Agent Chat，再单独评估演进，而不是提前为未来实现抽象。

## 12. OpenCode ACP 直接证据

### 12.1 stdio entry

文件：

```text
~/fork-code/opencode/packages/opencode/src/cli/cmd/acp.ts
```

重点：

- `AcpCommand` command 名为 `acp`：约 9–18 行。
- `AgentSideConnection`：约 55–61 行。
- stdin/stdout NDJSON stream：约 32–55 行。
- stdin end 决定 process lifetime：约 63–71 行。

结论：AssetIWeave 关闭 protocol stdin 后，OpenCode 正常路径应退出；仍需 process tree kill 兜底。

### 12.2 initialize capabilities

文件：

```text
~/fork-code/opencode/packages/opencode/src/acp/service.ts
```

约 110–136 行：

- protocolVersion = 1；
- loadSession = true；
- prompt image / embedded context；
- session capabilities 包含 close/fork/list/resume；
- 提供 auth method 与 agent info。

Phase 1 只使用 text、close。

### 12.3 session/new

同文件约 161–207 行：

- 依据 cwd 读取 directory snapshot；
- 选择 default model 和 mode；
- 创建 backing session；
- 返回 `sessionId` 与 `configOptions`。

结论：空 workspace 会影响项目级 config discovery，这是刻意隔离；用户级认证/配置仍需 smoke test。

### 12.4 cancel/close/model

同文件：

- `closeSession`：约 339–347 行。
- `cancel`：约 349–352 行。
- `setSessionConfigOption` model：约 398–420 行。
- `setSessionModel`：约 465–478 行。
- `prompt`：约 492 行开始。

结论：

- OpenCode 支持 session close。
- model selection 有 validation，失败必须上抛。
- cancel abort backing session。

## 13. 代码借鉴清单

### 可以按思想重写

- Registry HashMap + lookup。
- one-shot stdio ownership。
- stderr bounded tail。
- connection background actor + handle。
- initialize local timeout。
- prompt/cancel concurrency。
- safe spawn preview。
- close then kill/wait cleanup 顺序。
- event normalization boundary。

### 禁止整段搬入

- AionCore repository/DB catalog。
- full `AgentStreamEvent`。
- Session persistence/reconcile/recovery。
- terminal handlers。
- multi-session manager。
- process pool/suspend。
- Team/Workflow/MCP/Skill injection。
- CodeBuddy dialect shim。

## 14. 实现者最小阅读包

### Registry 任务

只读：

```text
AionCore registry.rs: AgentRegistry, get, decode_row
AionCore migration 001: OpenCode seed
AssetIWeave ai_execution.rs: resolve_cli_executable* helpers
```

### Process 任务

只读：

```text
AionCore cli_process/mod.rs: struct, take_stdio, kill, wait, stderr
AionCore cli_process/spawn_sdk.rs: spawn_for_sdk
AssetIWeave host_process.rs
```

### ACP Protocol 任务

只读：

```text
AionCore protocol/acp.rs: connect, run_sdk_background, Phase 1 methods
OpenCode acp/service.ts: initialize/new/cancel/close/model/prompt
```

### Aggregator 任务

只读：

```text
AionCore protocol/events/translate.rs
01-product-requirements.md FR-EVT
04-acp-process-runtime-design.md event section
```

### Translation integration 任务

只读：

```text
AssetIWeave card_translation.rs
AssetIWeave application/card_translation.rs
AssetIWeave Tauri/Engine translation command registration
05-translation-task-api-integration.md
```
