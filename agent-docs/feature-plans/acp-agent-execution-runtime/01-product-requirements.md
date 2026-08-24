# 需求规格：ACP Agent 执行运行时 (Product Requirements)

> Phase 1: OpenCode Translation

| 字段 | 值 |
|---|---|
| 状态 | Implemented |
| 版本 | 0.1.0 |
| 日期 | 2026-08-13 |
| 所属系统 | AssetIWeave Desktop / Rust Engine |
| 主要实现语言 | Rust 2021 |
| 首个消费者 | Conversation Card Translation |
| 首个 Agent | OpenCode |
| 首个协议后端 | ACP over stdio |
| 评审状态 | Accepted / Phase 1 Complete |

## 1. 文档约定

本文使用以下规范性术语：

- **MUST / 必须**：实现与验收不可省略。
- **MUST NOT / 禁止**：实现不得出现。
- **SHOULD / 应当**：默认采用；偏离时必须记录原因。
- **MAY / 可以**：可选能力，不影响本阶段验收。

本文是 Phase 1 的需求与架构基线，不是完整 Agent Chat Host 的设计。

## 2. 摘要

AssetIWeave 当前通过 `AiCliRuntime` 拼接不同 CLI 的私有参数并执行一次性命令，例如：

```text
Translation -> AiExecution -> AiCliRuntime -> opencode run
```

本功能建立统一的 Agent Execution Runtime，使业务只提交标准化 AI 执行请求，由 Runtime 根据 `AgentDefinition` 选择协议后端。Phase 1 以 OpenCode 的原生 ACP stdio 服务验证端到端链路：

```text
Translation
    -> AiExecutionRequest
    -> AgentExecutor
    -> AgentRegistry
    -> AgentDefinition(protocol = acp)
    -> AcpExecutionBackend
    -> ManagedAgentProcess
    -> opencode acp
```

Phase 1 只返回最终文本，不引入持久化 Agent Session、Agent Chat、Memory、Workflow 或完整流式事件模型。

## 3. 背景

AssetIWeave 中存在两个方向不同的第三方 Agent 集成：

```text
Conversation Ingestion                  AI Execution
         |                                   |
         v                                   v
Conversation Adapter                   Agent Runtime
         |                                   |
         v                                   v
File / DB / Web                             ACP
         |                                   |
         v                                   v
Historical Data                          Agent CLI
```

### 3.1 Conversation Adapter

Conversation Adapter 回答“第三方 Agent 过去产生了什么数据”。它继续使用现有插件化采集、标准化和持久化架构。本 Spec 不修改该域。

### 3.2 Agent Execution Runtime

Agent Execution Runtime 回答“AssetIWeave 如何主动调用第三方 Agent 完成 AI Task”。它负责 Agent 定义、路由、进程、协议、生命周期、取消、错误与结果归一化。

### 3.3 当前实现基线

截至 AssetIWeave `04bc5de4babcbab0942c49913c8db6087e9a89d7`：

- `backend/ai_execution.rs` 定义 `AiCliRuntime::{Opencode, Gemini}`。
- `execute_structured_text` 根据 Vendor 枚举拼接 `opencode run` 或 `gemini --prompt` 参数。
- `backend/card_translation.rs` 直接选择 `AiCliRuntime`。
- `backend/host_process.rs` 已支持一次性进程的超时、输出上限、取消、Unix process group 和 Windows `taskkill /T`。
- Tauri Translation 命令通过 `spawn_blocking` 等待最终结果。
- Engine contract 已暴露 Translation availability、run、connection-test 和 model-list 方法。

该实现适合少量 CLI 集成，但 Vendor、调用参数和业务流程耦合，不适合扩展为统一 Agent Runtime。

## 4. 参考实现与证据基线

### 4.1 参考仓库

AionUi 的 Agent Runtime 核心实际位于 AionCore。本 Spec 参考本地检出的：

```text
~/fork-code/AionCore
commit 4bfd7e2cd6d6b6b3371e0b99525143cefc554c86
```

重点代码：

| 能力 | AionCore 位置 |
|---|---|
| Agent Registry | `crates/aionui-ai-agent/src/registry.rs` |
| Runtime 路由 | `crates/aionui-ai-agent/src/factory/acp.rs` |
| ACP SDK wrapper | `crates/aionui-ai-agent/src/protocol/acp.rs` |
| CLI Process | `crates/aionui-ai-agent/src/capability/cli_process/` |
| ACP Session Flow | `crates/aionui-ai-agent/src/manager/acp/agent_session_flow.rs` |
| Event normalize | `crates/aionui-ai-agent/src/protocol/events/` |
| ACP dialect shim | `crates/aionui-ai-agent/src/protocol/acp_dialect.rs` |
| 新 Session Backend seam | `crates/aionui-session/src/backend/` |

### 4.2 已确认的参考事实

- AionCore 当前使用 `agent-client-protocol = 2.0.0`。
- AionCore 通过 `AgentRegistry` 统一读取 Agent metadata，并按 Backend Route 选择 ACP 或其他 Runtime。
- AionCore 将 OS Process 与 ACP Protocol 分离，ACP 只接管 child stdin/stdout。
- AionCore 在 SDK 后台 task 中保持同一 ACP connection，并通过该 connection 发送 session operations。
- AionCore 的 OpenCode 内置定义为 `command = "opencode"`、`args = ["acp"]`。
- OpenCode 的 ACP initialize capabilities 声明 `session/close`，并实现 `session/new`、`session/prompt`、`session/cancel`、model selection 与 session close。

### 4.3 借鉴边界

本阶段只借鉴：

1. Agent 定义数据驱动。
2. Runtime 按协议路由，而不是按 Vendor 路由。
3. Protocol 与 OS Process 分离。
4. Transport 对业务密封。
5. 标准 ACP 优先，Vendor 方言兼容最小化。

本阶段不复制 AionCore 的多 crate 组织、完整 Chat Host、Session 持久化或 Team Runtime。

## 5. 目标

### 5.1 产品目标

为 AssetIWeave 提供一个可扩展的 Agent Execution Runtime，并以 OpenCode Translation 证明业务可以在不知道 ACP wire shape 和 OpenCode 私有执行参数的情况下完成 AI 文本任务。

### 5.2 工程目标

1. Translation MUST 依赖 `AiExecution`，不得直接依赖 ACP 或 OpenCode process 参数。
2. Agent lookup、command、args、env 和 protocol MUST 来自 `AgentRegistry`。
3. Vendor 扩展 MUST 通过新增或修改 `AgentDefinition` 完成，不得在业务层增加 Vendor `match`。
4. ACP Protocol MUST 使用 typed ACP SDK，不得自行维护一套通用 JSON-RPC 实现。
5. ACP Protocol MUST NOT 直接负责 OS process tree 管理。
6. Runtime MUST 支持超时、取消、输出上限和确定性清理。
7. OpenCode Translation MUST 从 `opencode run` 迁移到 `opencode acp`。
8. 当前 Gemini Translation MUST 在 Phase 1 中保持可用，除非评审明确决定同步迁移或移除。
9. 长耗时执行 MUST 不持有全局 app lock，并且桌面端必须具备后台任务快照与恢复观察能力。

### 5.3 成功定义

在安装并完成认证的 OpenCode 环境中，用户发起 Conversation Card Translation 后：

1. Runtime 从 Registry 解析 `opencode`。
2. 启动 `opencode acp` 长生命周期子进程。
3. 完成 ACP initialize 与临时 session/new。
4. 可选应用用户指定 model。
5. 发送 translation prompt。
6. 聚合 assistant text。
7. 返回并保存翻译结果。
8. close session、终止并回收完整进程树。

整个过程中 UI 的导航、筛选、设置和无冲突操作保持可用。

## 6. 非目标

Phase 1 明确不实现：

- Conversation 与 Agent Session 的持久绑定。
- `session/list`、`session/load`、`session/resume`、`session/fork`。
- 多轮 Agent Chat。
- Agent Team、Workflow、Cron 或 Orchestrator。
- Memory 作为 AiExecution 消费者。
- MCP server 注入、Skill 注入。
- Permission UI、Terminal UI。
- 完整 `AgentStreamEvent` 公共模型。
- Long-lived Agent Pool 或跨请求 connection reuse。
- Session suspend、rehydrate 或 crash recovery。
- Vendor-specific ACP Adapter 层。
- Agent definition 的用户编辑 UI。
- Agent Registry marketplace。

## 7. 假设与默认决策

以下默认决策适用于 Proposed 版本；评审可以修改：

| ID | 默认决策 |
|---|---|
| D-001 | Phase 1 每次 execution 启动一个独立 OpenCode ACP process，不建立 process pool。 |
| D-002 | Phase 1 的 Agent Registry 使用代码内置定义，不新增 SQLite 表。 |
| D-003 | OpenCode execution 使用 ACP；Gemini 暂时保留现有 legacy CLI 路径。 |
| D-004 | 用户显式指定 model 时，model 设置失败即整个 execution 失败，不静默回退。 |
| D-005 | Permission request 一律拒绝；发现 tool call activity 时取消并以策略错误结束。 |
| D-006 | Translation 在 app-owned 空临时目录中执行，不继承用户当前项目目录。 |
| D-007 | Desktop 使用后台任务快照；Engine 的现有同步 command 保持兼容。 |
| D-008 | Phase 1 采用 ACP SDK 2.0.x，并在实现时固定兼容版本。 |
| D-009 | 本阶段不实现 ACP dialect shim；真实兼容性证据出现后再添加。 |

## 8. 术语

| 术语 | 定义 |
|---|---|
| Agent | 可由 AssetIWeave 主动调用、执行 AI task 的外部程序。 |
| Agent Definition | Agent 的静态启动与协议定义。 |
| Agent Registry | Agent Definition 的唯一查询入口。 |
| Agent Executor | 解析 Definition、选择 Runtime Backend 并协调 execution 生命周期的组件。 |
| Runtime Backend | 封装一种 Agent transport，例如 ACP 或未来 Native。 |
| ACP Protocol | Agent Client Protocol 的 typed client connection 与 session operations。 |
| Managed Agent Process | 可交出 stdin/stdout、监控 stderr、等待并终止完整进程树的长生命周期子进程。 |
| Execution | 一次从 request 到 result/error 的独立 AI task。 |
| Temporary Session | 仅服务一次 execution，完成后关闭且不持久化的 ACP session。 |

## 9. 用户场景

### UC-001 翻译内容卡片

用户点击翻译按钮，系统使用设置中选择的 OpenCode 和可选 model 翻译文本，并将最终文本保存到对应 Conversation Part。

### UC-002 测试 Translation 连接

用户在设置中执行连接测试。系统复用相同 Agent Runtime 完成一个短 prompt，而不是调用另一条 OpenCode 私有执行路径。

### UC-003 取消运行中的翻译

用户取消任务或 app 开始关闭时，系统发送 ACP cancel，随后在有界 grace period 后清理 session 与完整进程树。

### UC-004 Agent 不可用

OpenCode 不在 PATH 或 login shell 可发现路径中时，Registry 返回结构化 unavailable reason，UI 展示可行动错误。

## 10. 功能需求

### 10.1 Agent Definition 与 Registry

#### FR-REG-001 Agent Definition

系统 MUST 定义至少包含以下字段的 `AgentDefinition`：

```rust
pub(crate) struct AgentDefinition {
  pub(crate) id: AgentId,
  pub(crate) protocol: AgentProtocol,
  pub(crate) command: String,
  pub(crate) args: Vec<String>,
  pub(crate) env: Vec<AgentEnvEntry>,
  pub(crate) capabilities: DeclaredAgentCapabilities,
  pub(crate) probe: Option<AgentProbeDefinition>,
  pub(crate) model_discovery: Option<AgentCommandDefinition>,
}
```

Phase 1 MUST 包含：

```text
id       = opencode
protocol = acp
command  = opencode
args     = ["acp"]
```

#### FR-REG-002 唯一查询入口

所有 execution MUST 通过 `AgentRegistry::get(agent_id)` 获取定义。Translation、Tauri command 和 Engine adapter MUST NOT 自行构造 OpenCode command/args。

#### FR-REG-003 Vendor 隔离

业务层禁止出现：

```rust
match agent_id {
  "opencode" => ...,
  "qwen" => ...,
}
```

`AgentExecutor` MAY 对 `AgentProtocol::{Acp, Native}` 做协议级路由。

#### FR-REG-004 Availability

Registry MUST 提供 availability probe，至少区分：

- available；
- executable not found；
- probe timeout；
- probe failed；
- invalid definition。

可执行文件解析 SHOULD 复用并泛化当前 PATH、login shell 和常见安装目录查找逻辑。

#### FR-REG-005 Runtime observation

ACP initialize 返回的 capabilities MAY 缓存为进程内 observation，但 Phase 1 MUST NOT 为此新增持久化 schema。

### 10.2 AiExecution 与 AgentExecutor

#### FR-EXE-001 标准请求

Runtime MUST 接受与 Vendor、ACP 解耦的请求：

```rust
pub(crate) struct AiExecutionRequest {
  pub(crate) agent_id: AgentId,
  pub(crate) prompt: String,
  pub(crate) model: Option<String>,
  pub(crate) purpose: AiExecutionPurpose,
  pub(crate) limits: AiExecutionLimits,
  pub(crate) cancellation: AiExecutionCancellation,
}
```

`purpose` Phase 1 至少包含 `Translation`，只用于策略、日志与临时 workspace 选择，不用于 Vendor 路由。

#### FR-EXE-002 标准结果

成功结果 MUST 至少包含：

```rust
pub(crate) struct AiExecutionResult {
  pub(crate) text: String,
  pub(crate) agent_id: AgentId,
  pub(crate) protocol: AgentProtocol,
  pub(crate) model: Option<String>,
  pub(crate) elapsed_ms: u64,
}
```

公开给现有 Translation API 时，可继续映射为 `translated_text`，避免前端无关改动。

#### FR-EXE-003 输入校验

Runtime MUST 在启动进程前校验：

- prompt trim 后非空；
- prompt 不超过 1,000,000 bytes；
- Translation 自身可保留更严格的 200,000-byte 上限；
- model 最长 120 bytes；
- model 不包含 `\n`、`\r` 或 NUL；
- agent id 存在且 enabled。

#### FR-EXE-004 协议路由

`AgentExecutor` MUST 只依据 `AgentDefinition.protocol` 选择 backend。Phase 1 实现 `AcpExecutionBackend`；`Native` 可以保留类型与明确的 unsupported error，但不得为 OpenCode 使用 Native。

#### FR-EXE-005 并发限制

Desktop Runtime MUST 对同时运行的 Agent execution 设置有界并发。Phase 1 默认最大并发数为 2；超出请求进入可取消队列，不得无界 spawn process。

### 10.3 Managed Agent Process

#### FR-PROC-001 长生命周期 stdio

`ManagedAgentProcess` MUST 支持：

- spawn child；
- piped stdin/stdout/stderr；
- 将 stdin/stdout 所有权一次性交给 ACP Protocol；
- 独立有界读取 stderr；
- `try_wait` / `wait`；
- kill process tree；
- reap child；
- 获取 pid、exit status 与截断后的 stderr diagnostic。

#### FR-PROC-002 Process tree

- Unix MUST 将 child 放入独立 process group，并按 process group 终止。
- Windows MUST 优先终止完整 child tree，并以 direct child kill 作为 fallback。
- 终止 MUST 最终执行 `wait`，避免 zombie process。

#### FR-PROC-003 职责边界

`ManagedAgentProcess` MUST NOT 解析 ACP；ACP Protocol MUST NOT 自行调用 Vendor command。

#### FR-PROC-004 stderr 上限

stderr reader MUST 持续 drain，保留内容默认上限为 256 KiB，并记录 truncated 状态，防止 pipe backpressure 与无界内存增长。

### 10.4 ACP Protocol

#### FR-ACP-001 SDK

实现 MUST 使用 `agent-client-protocol` typed SDK。禁止把逐行 JSON 手工解析作为正常 ACP 实现。

#### FR-ACP-002 Phase 1 methods

Phase 1 只要求：

```text
initialize
session/new
session/prompt
session/cancel
session/close
session/set_model（仅在 model 非空时）
```

不要求 list/load/resume/fork。

#### FR-ACP-003 initialize

`connect` MUST：

1. 接管 child stdin/stdout；
2. 在后台 task 运行 ACP connection actor；
3. 发送包含 AssetIWeave client info 的 initialize；
4. 使用独立 initialize timeout，默认 10 秒；
5. 缓存 initialize response 与 capabilities；
6. 在失败时返回结构化 handshake error。

#### FR-ACP-004 session/new

Runtime MUST 使用 app-owned 临时 workspace 的绝对路径作为 `cwd`，并传入空 `mcp_servers`。返回的 `session_id` 只保存在当前 execution 内存中。

#### FR-ACP-005 model

当 request.model 非空：

1. Runtime MUST 在 `session/new` 后、`session/prompt` 前设置 model。
2. model RPC timeout 默认 5 秒。
3. 不支持、找不到或设置失败 MUST 返回 `ModelSelectionFailed`。
4. Runtime MUST NOT 静默使用默认 model。

#### FR-ACP-006 prompt completion

`session/prompt` response 表示 turn 完成。Runtime MUST 同时消费 `session/update`，只聚合属于当前 session 的 assistant text chunk。

#### FR-ACP-007 connection concurrency

ACP connection actor MUST 独立于调用 `prompt` 的 future 运行，使 `session/cancel` 可以在 prompt 尚未完成时发送。

#### FR-ACP-008 close capability

OpenCode advertise `session/close` 时 MUST 尝试 close。对未来未 advertise close 的 ACP Agent，Runtime SHOULD 跳过 close RPC 并继续完成 process cleanup。

#### FR-ACP-009 Dialect shim

Phase 1 MUST 使用标准 ACP 路径。只有出现可复现、已记录的 Vendor wire incompatibility 时，才允许在 Protocol transport 边界增加只处理该差异的 thin shim。

### 10.5 Event aggregation 与执行策略

#### FR-EVT-001 文本聚合

Aggregator MUST：

- 只收集 assistant message 的 text content；
- 保持 chunk 顺序；
- 忽略 thinking content，但 MAY 记录 debug event type；
- 不把原始 ACP JSON 返回给 Translation；
- 强制 1 MiB 最终文本上限；
- trim 最终文本，并拒绝空结果。

#### FR-EVT-002 Permission

Translation policy MUST 自动拒绝所有 ACP permission request。拒绝必须在协议要求的响应期限内完成，不得等待 UI。

#### FR-EVT-003 Tool activity

Translation 不需要工具。发现 tool call start/update 时 Runtime MUST：

1. 记录不含敏感参数的 policy event；
2. 发送 session cancel；
3. 返回 `ToolUseDenied`；
4. 执行统一 cleanup。

#### FR-EVT-004 Session correlation

Aggregator MUST 丢弃或诊断不属于当前 `session_id` 的 update，禁止跨 execution 混入文本。

### 10.6 取消、超时与清理

#### FR-LIFE-001 总超时

Translation execution 默认总超时为 180 秒。initialize、model RPC、close 各自还 MUST 有更短的局部 timeout。

#### FR-LIFE-002 取消

收到用户取消、app close 或 task cancellation 后：

1. 若 session 已创建，发送 `session/cancel`；
2. 等待最多 2 秒 grace period；
3. 尝试 `session/close`；
4. 关闭 protocol actor；
5. 终止完整 process tree；
6. wait/reap child；
7. 删除临时 workspace；
8. task 状态收敛为 `cancelled`。

#### FR-LIFE-003 Cleanup guarantee

成功、失败、超时、取消、protocol disconnect、output overflow 和 panic boundary 均 MUST 进入同一 cleanup path。Cleanup MUST 是幂等的。

#### FR-LIFE-004 App close

App close 检测 MUST 将 Agent execution 计入 running background tasks，并允许用户确认中断。确认退出后必须触发取消和 process cleanup。

### 10.7 Desktop 后台任务

#### FR-TASK-001 快速返回

Desktop 启动 Translation 时，Tauri command SHOULD 在 100 ms 级别返回 `AiExecutionTaskSnapshot`，不得等待完整 AI execution。

#### FR-TASK-002 Task snapshot

Snapshot MUST 至少包含：

```rust
pub(crate) struct AiExecutionTaskSnapshot {
  pub(crate) id: String,
  pub(crate) purpose: AiExecutionPurpose,
  pub(crate) agent_id: AgentId,
  pub(crate) state: AiExecutionTaskState,
  pub(crate) phase: AiExecutionPhase,
  pub(crate) created_at: String,
  pub(crate) updated_at: String,
  pub(crate) result: Option<AiExecutionResult>,
  pub(crate) error: Option<AiExecutionErrorView>,
}
```

状态至少包含 `queued | running | succeeded | failed | cancelled`；phase 至少包含 `resolving | spawning | initializing | creating_session | configuring | prompting | closing | cleaning_up`。

#### FR-TASK-003 观察与取消

Desktop adapter MUST 暴露 start、get/list 和 cancel。前端 MUST 监听 task update event，并以 polling 作为 missed-event fallback。

#### FR-TASK-004 UI 可用性

执行中只禁用当前 block 的冲突操作。导航、过滤、浏览、设置和其他 block 操作 MUST 保持可用。离开发起页面后，全局后台任务区域仍 MUST 显示运行状态。

#### FR-TASK-005 Result retention

Phase 1 task/result 只保存在内存中；完成快照默认保留 10 分钟或最近 100 条，以先达到者触发淘汰。Prompt MUST NOT 保存在 task snapshot。

### 10.8 Translation 集成

#### FR-TR-001 OpenCode 路径

OpenCode Translation、connection test 与实际 Translation execution MUST 复用同一 `AgentExecutor`。实际 Translation 路径禁止调用 `opencode run`。

#### FR-TR-002 现有公开结果兼容

现有 `OpencodeTranslationResult { translated_text }` MAY 在 Phase 1 保留为 adapter DTO，由 application 层从 `AiExecutionResult.text` 映射。

#### FR-TR-003 Gemini 兼容

在 Gemini 尚未进入 Agent Registry/Native Backend 前，现有 Gemini Translation 行为 MUST 保持不变。该临时 legacy seam MUST：

- 不被 OpenCode 使用；
- 有明确代码注释和后续迁移任务；
- 不扩展到新的 Agent；
- 不泄漏到新的 `AgentExecutor` 接口。

#### FR-TR-004 Model discovery

现有 OpenCode model list 命令 MAY 作为 `AgentDefinition.model_discovery` 的数据驱动 probe 保留。业务层不得硬编码 `opencode models`。

#### FR-TR-005 Translation persistence

Runtime 只返回结果；将翻译写入 Conversation Part 继续由 Conversation application workflow 负责。Runtime MUST NOT 直接写 Conversation 表。

### 10.9 Tauri、Engine 与 CLI 边界

#### FR-API-001 AppService

共享 execution workflow MUST 通过 `AppService` 或其持有的 application capability 进入。Tauri 页面、frontend service 和 CLI 不得绕过该边界。

#### FR-API-002 Engine compatibility

Engine 的 `conversation.card.translation.run` MUST 继续可用，并使用与 Desktop 相同的 AgentExecutor。由于 CLI Engine 生命周期是一次请求，Engine MAY 同步等待结果；不得复制协议或 process 实现。

#### FR-API-003 Contract regeneration

任何 Engine DTO、method、risk 或 exposure 变化后 MUST 运行：

```bash
pnpm cli:contract
```

禁止手工编辑 `cli/internal/schema/contract.json`。

## 11. 非功能需求

### NFR-001 可靠性

- 所有终态必须回收 child。
- timeout/cancel 后 5 秒内不得残留该 execution 的 OpenCode process tree。
- event channel 关闭不得造成死锁。
- stderr 大量输出不得阻塞 ACP stdout。

### NFR-002 性能

- Registry in-memory lookup p95 < 1 ms。
- Desktop start command 在不含 OS 调度极端情况下 p95 < 100 ms。
- Runtime 自身除 Agent 响应外的额外启动开销应可观测；Phase 1 不规定硬性总延迟 SLA。
- 最大并发为 2，队列有界且可取消。

### NFR-003 安全与隐私

- Translation 默认使用空的 app-owned 临时 workspace。
- `session/new` 不注入 MCP server。
- Permission request 自动拒绝。
- 日志禁止记录 prompt、translated text、环境变量值、认证信息和完整 ACP payload。
- command、args 和 env 来自受信任的内置 AgentDefinition，不接受前端任意 command path。
- 临时目录必须位于 app cache/runtime 目录，并在终态删除。

### NFR-004 可移植性

Process lifecycle MUST 覆盖 macOS、Linux 和 Windows。平台相关逻辑集中在 process 层，不得进入 ACP backend。

### NFR-005 可维护性

- 新增标准 ACP Agent 不应修改 Translation 业务代码。
- ACP wire 类型只出现在 `agents/protocol/acp.rs` 与 ACP backend 内。
- 单个模块不同时承担 Registry、Process、Protocol 和业务 mapping。

### NFR-006 可观测性

每次 execution MUST 生成 `execution_id`，结构化记录：

- agent id、protocol、phase；
- process spawn success/failure 与 pid；
- initialize/session/prompt/cleanup 的耗时；
- timeout、cancel、disconnect、tool denied、output overflow；
- exit status、stderr truncated 标记。

日志 MUST 使用 payload 长度、chunk 数量或 hash 等安全元数据，不得写入原始内容。

## 12. 目标架构

```text
Frontend Translation UI
          |
          v
frontend/src/services/cardTranslation.ts
          |
          v
Tauri / Engine Adapter
          |
          v
AppService: Translation workflow
          |
          v
AiExecutionRequest
          |
          v
AgentExecutor ---------> AgentRegistry
          |                    |
          |                    v
          |              AgentDefinition
          |                    |
          +<-------------------+
          |
          v
route by AgentProtocol
          |
    +-----+------+
    |            |
    v            v
ACP Backend   Native Backend (future)
    |
    +-----------> AcpProtocol
    |                 |
    +-----------> ManagedAgentProcess
                          |
                          v
                    host_process / OS
```

### 12.1 依赖方向

```text
card_translation
    -> ai_execution
        -> agents::registry
        -> agents::protocol abstraction

acp execution backend
    -> agents::protocol::acp
    -> agents::process

agents::process
    -> host_process platform primitives
```

反向依赖禁止：Registry、Process、ACP Protocol 不得依赖 Translation。

### 12.2 组件职责

| 组件 | 职责 | 不负责 |
|---|---|---|
| Translation workflow | 生成 prompt、调用 AiExecution、保存结果 | Agent lookup、ACP、process |
| AgentExecutor | 校验、lookup、协议路由、生命周期协调 | UI DTO、Conversation persistence |
| AgentRegistry | definition、availability、runtime observation | 执行 prompt |
| AcpExecutionBackend | 一次 ACP execution 的 session flow | OS process tree 细节 |
| AcpProtocol | ACP typed connection 与 method | command discovery、业务策略 |
| ManagedAgentProcess | child stdio、stderr、wait、kill tree | ACP message parsing |
| Task Registry | Desktop task state、event、cancel、retention | AI 协议实现 |

## 13. 执行流程

```text
validate request
    |
    v
acquire bounded execution permit
    |
    v
AgentRegistry.get(agent_id)
    |
    v
resolve executable + create empty temp workspace
    |
    v
ManagedAgentProcess.spawn(command, args=["acp"])
    |
    +---- stderr bounded drain
    |
    v
AcpProtocol.connect(stdin, stdout)
    |
    v
initialize
    |
    v
session/new(cwd=temp, mcp_servers=[])
    |
    +---- model? -> session/set_model
    |
    v
session/prompt(text prompt)
    |
    +---- session/update -> aggregate assistant text
    +---- permission -> deny
    +---- tool activity -> cancel + fail
    |
    v
validate final text
    |
    v
session/close
    |
    v
shutdown protocol + kill/wait child + delete temp
    |
    v
AiExecutionResult
```

## 14. 生命周期状态机

```text
queued
  -> resolving
  -> spawning
  -> initializing
  -> creating_session
  -> configuring      (optional model)
  -> prompting
  -> closing
  -> cleaning_up
  -> succeeded

Any non-terminal state
  -> cancelling
  -> cleaning_up
  -> cancelled

Any non-terminal state
  -> failing
  -> cleaning_up
  -> failed
```

规则：

- `succeeded` 只能在 cleanup 完成后发布。
- `failed` 和 `cancelled` 也只能在 cleanup 完成后发布。
- Terminal snapshot 不得再次回到 running state。
- 多次 cancel 必须幂等。

## 15. 错误模型

Runtime MUST 使用可分类错误，而不是只返回字符串。至少包括：

| Code | 含义 | Retryable |
|---|---|---|
| `agent_not_found` | Registry 无该 agent | 否 |
| `agent_unavailable` | executable 不可用 | 条件性 |
| `invalid_request` | prompt/model/limit 非法 | 否 |
| `queue_full` | 有界队列已满 | 是 |
| `spawn_failed` | child 启动失败 | 条件性 |
| `handshake_timeout` | initialize 超时 | 是 |
| `protocol_error` | ACP request/response 错误 | 条件性 |
| `session_create_failed` | session/new 失败 | 条件性 |
| `model_selection_failed` | 指定 model 无法应用 | 否/条件性 |
| `permission_denied` | Agent 请求了不允许的权限 | 否 |
| `tool_use_denied` | Translation 发生 tool activity | 否 |
| `output_limit` | 最终文本超过上限 | 否 |
| `empty_output` | 无 assistant text | 条件性 |
| `timeout` | 总执行超时 | 是 |
| `cancelled` | 用户或 app 取消 | 否 |
| `agent_exited` | child 非预期退出 | 条件性 |
| `cleanup_failed` | 清理未完全成功 | 是，且高优先级诊断 |

对外错误 view MUST 包含稳定 code、用户可读 message、retryable 和可选安全 details；不得暴露完整 stderr 或 prompt。

## 16. 目录与模块规划

```text
src-tauri/src/backend/
├── ai_execution/
│   ├── mod.rs
│   ├── types.rs
│   ├── error.rs
│   ├── executor.rs
│   ├── task.rs
│   └── backends/
│       ├── mod.rs
│       ├── acp.rs
│       └── acp_aggregator.rs
├── agents/
│   ├── mod.rs
│   ├── types.rs
│   ├── registry.rs
│   ├── process.rs
│   └── protocol/
│       ├── mod.rs
│       └── acp.rs
├── card_translation.rs
└── host_process.rs
```

说明：

- 现有 `ai_execution.rs` 迁移为目录模块。
- `host_process.rs` 保留平台级 process primitives；`agents/process.rs` 组合成长生命周期 Agent process。
- Phase 1 不创建空壳 `native.rs`；只在 types 中保留 protocol variant 和 unsupported error。
- Task registry 可复用现有 background task 基础设施；不得创建第二套相同语义的全局任务系统。

## 17. Code Style

遵循仓库现有 Rust 风格、`rustfmt` 和结构化错误。示例：

```rust
pub(crate) async fn execute(
  &self,
  request: AiExecutionRequest,
) -> Result<AiExecutionResult, AiExecutionError> {
  request.validate()?;
  let definition = self.registry.require(&request.agent_id)?;

  match definition.protocol {
    AgentProtocol::Acp => self.acp.execute(&definition, request).await,
    AgentProtocol::Native => Err(AiExecutionError::UnsupportedProtocol {
      protocol: definition.protocol,
    }),
  }
}
```

约束：

- 类型使用 `PascalCase`，函数与字段使用 `snake_case`。
- 业务错误不得依赖解析英文 stderr 文本来决定控制流。
- Vendor ID 只能出现在内置 definition、测试 fixture 和兼容迁移位置。
- 不创建 `legacy/`、`new/` 或 `v2/` 平行目录树；Gemini 临时 seam 使用明确命名与删除条件。

## 18. 测试策略

### 18.1 单元测试

必须覆盖：

- Registry lookup、重复 ID、invalid definition。
- protocol route 不依赖 Vendor ID。
- request prompt/model 校验。
- assistant text aggregation、session correlation、empty output、output cap。
- permission auto-deny 与 tool activity failure。
- error mapping 不泄漏 prompt/stderr。
- task state transition、cancel idempotence、retention 与 concurrency limit。

### 18.2 Process 集成测试

使用本地 fixture child 覆盖：

- stdin/stdout ownership transfer；
- stderr 大量输出持续 drain；
- timeout、cancel、unexpected exit；
- Unix process group / Windows child tree cleanup；
- kill 后 wait/reap；
- cleanup 多次调用。

### 18.3 Fake ACP Agent 集成测试

必须提供可控的本地 fake ACP stdio agent，覆盖：

- initialize -> new -> prompt -> close happy path；
- initialize timeout；
- prompt streaming chunks；
- wrong session update；
- permission request；
- tool activity；
- cancel during prompt；
- model success/unsupported/failure；
- process exit before prompt completion；
- close failure后仍完成 process cleanup。

测试不得依赖真实网络或用户 OpenCode 配置。

### 18.4 AppService / Engine contract 测试

必须证明：

- Translation 映射到 `agent_id = opencode`。
- OpenCode execution 不生成 `run` 参数。
- Engine 与 Tauri 使用相同 AgentExecutor core。
- contract regeneration 后 schema 与 Rust DTO 一致。

### 18.5 Frontend 测试

必须覆盖：

- start 后显示当前 block 进度；
- task event 与 polling fallback；
- cancel；
- 成功后保存 translated text；
- 失败显示稳定错误；
- 运行中无关 UI 可交互；
- 离开页面后全局任务提示仍可见。

### 18.6 Real OpenCode smoke test

在已安装、已认证 OpenCode 的开发机上手动验证：

1. `opencode --version` 可用；
2. Runtime 实际启动 `opencode acp`；
3. 指定与未指定 model 各完成一次 Translation；
4. cancel 后无残留 process；
5. app close 时正确提示并清理；
6. 日志不包含 prompt 与 translation content。

## 19. 验证命令

### Rust

```bash
cargo fmt --all -- --check
cargo test --workspace
```

开发阶段可先运行定向测试：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution
cargo test --manifest-path src-tauri/Cargo.toml backend::card_translation
```

### Frontend

```bash
pnpm typecheck
pnpm test
pnpm build
```

### Engine / CLI

```bash
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

### Desktop manual

```bash
pnpm tauri:dev
```

## 20. 实施边界

### Always / 必须执行

- 修改前先更新本 Spec 中受影响的 contract。
- 所有 execution terminal path 执行 cleanup。
- 测试使用临时 `ASSETIWEAVE_DB_PATH`。
- 长耗时工作不持有全局 app lock。
- Engine contract 变化后重新生成 contract。
- 依赖版本与兼容性证据记录在提交或 ADR 中。

### Ask First / 需先评审

- 新增 Agent Registry SQLite schema。
- 将 Gemini 一并迁移到 Native Backend。
- 引入 process pool 或跨 execution session reuse。
- 引入 ACP dialect shim。
- 允许 Translation tool/MCP/项目 workspace 访问。
- 改变现有公开 Engine method 名称或删除 DTO 字段。
- 修改默认并发数、timeout 或 output cap。

### Never / 禁止

- 在 Translation 业务层硬编码 `opencode acp`。
- 让前端传入任意 executable、args 或 env。
- 手工编辑生成的 Engine contract。
- 把 prompt、result、token、API key 或完整 env 写入日志。
- 为单个 Vendor 复制完整 ACP adapter。
- 只 kill direct child 而不处理 process tree。
- 在 global app lock 内等待 Agent 或网络。
- 为本阶段实现 Session persistence、Chat、Team 或 Workflow。

## 21. 兼容与迁移

### 21.1 Strangler 顺序

1. 建立 Registry、Managed Process、ACP Protocol 和 AgentExecutor。
2. Fake ACP tests 通过后接入 OpenCode definition。
3. connection test 切到 AgentExecutor。
4. OpenCode Translation 切到 ACP。
5. 确认 Translation 范围无 `opencode run` execution call site。
6. 保留 Gemini legacy path。
7. 后续独立 Spec 决定 Gemini Native Backend 与 `AiCliRuntime` 最终删除。

### 21.2 回滚

实施结果：未建立 OpenCode Translation 双执行路径或 feature flag。T22 已用真实 OpenCode 完成成功、model、invalid-model、cancel、process cleanup 与日志脱敏 smoke，因此 T23 直接确认 ACP 为 Translation 唯一路径。既有 Memory AI 链路属于本 Spec 的明确非目标，并按用户决定留待 Memory 重写 Spec 处理。

### 21.3 数据迁移

Phase 1 Registry 为内置定义，task/result 为内存数据，因此不需要 SQLite migration。Conversation translation persistence schema 不变。

## 22. 验收标准

### AC-001 架构边界

- [x] Translation 不 import ACP SDK 类型。
- [x] Translation 不构造 OpenCode command/args。
- [x] AgentExecutor 只按 `AgentProtocol` 路由。
- [x] ACP Protocol 与 ManagedAgentProcess 为独立模块。

### AC-002 OpenCode ACP happy path

- [x] Registry 返回 `opencode + ["acp"]`。
- [x] 完成 initialize、session/new、prompt、close。
- [x] assistant text 正确聚合为 `translated_text`。
- [x] 结束后 child 与 descendants 全部退出。

### AC-003 Model

- [x] model 为空时使用 Agent 默认值。
- [x] model 非空时在 prompt 前成功应用。
- [x] 无效或不支持 model 不静默回退。

### AC-004 策略

- [x] permission 自动拒绝。
- [x] tool activity 导致 cancel 与 `tool_use_denied`。
- [x] session update 不跨 session 聚合。

### AC-005 生命周期

- [x] success、failure、timeout、cancel 均进入 cleanup。
- [x] cancel 5 秒内无残留 process tree。
- [x] stderr overflow 不阻塞 process。
- [x] close 失败不阻止 kill/wait。

### AC-006 后台任务与响应性

- [x] Desktop start 快速返回 task snapshot。
- [x] event + polling 均可恢复最新状态。
- [x] 当前 block 显示进度并可取消。
- [x] 无关 UI 控件在执行中保持可用。
- [x] app close 可识别 running Agent task。

### AC-007 兼容

- [x] Gemini Translation 行为不回归。
- [x] Engine Translation command 继续可用。
- [x] OpenCode Translation execution call site 不再使用 `opencode run`。
- [x] Conversation translation persistence 不变。

### AC-008 质量门

- [x] Rust、frontend、Go CLI tests 全部通过。
- [x] Engine contract 已重新生成且无手工修改。
- [x] Fake ACP agent 覆盖异常路径。
- [x] Real OpenCode smoke test 通过。
- [x] 日志抽查不含 prompt、result、env value 或 auth secret。

## 23. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| ACP SDK 2.0.x API 仍变化 | 编译或 wire 兼容失败 | 固定兼容版本；fake agent contract tests；升级独立提交 |
| OpenCode model RPC 兼容差异 | 指定 model 失败 | 对真实 OpenCode smoke test；显式失败，不静默回退 |
| Agent 自发使用工具 | 数据访问或非预期行为 | 空 workspace、空 MCP、deny permission、tool event 即取消 |
| Wrapper CLI 产生 descendants | 取消后残留进程 | 独立 process group / tree kill + wait/reap tests |
| 并发翻译启动过多 Agent | CPU/内存压力 | bounded concurrency = 2；队列有界、可取消 |
| Background task 增加前端范围 | 交付变大 | 复用现有 task provider/event/polling 模式，不新建平行框架 |
| Gemini 与新 Runtime 暂时并存 | 架构有过渡 seam | 限定只服务 Gemini；单独后续 Spec；禁止新增使用者 |
| 临时 workspace 改变 OpenCode 配置发现 | 与当前 CLI 行为不同 | 保留用户级认证/配置；真实 smoke test；不加载项目级规则是预期策略 |

## 24. 待评审问题

### OQ-001 Gemini 迁移边界

**推荐**：Phase 1 只迁移 OpenCode，Gemini 保持 legacy path；下一阶段用数据驱动 Native Backend 迁移 Gemini。

需要确认：是否接受短期双路径，还是要求 Phase 1 同时实现 Native Backend 并彻底删除 `AiCliRuntime`？

### OQ-002 Desktop API 形态

**推荐**：Tauri 使用 start/get/list/cancel task API；Engine 保持同步等待结果。两者共享 AgentExecutor，不共享 adapter 生命周期。

需要确认：Translation UI 是否需要首版就提供用户可见“取消”按钮和全局 Agent task indicator？本 Spec 按仓库长任务规则要求提供。

### OQ-003 Tool policy

**推荐**：Translation 严格 no-tool；一旦观察到 tool activity，取消并失败。

需要确认：是否允许只读工具，或 OpenCode 的网络工具？若允许，需要单独权限与数据边界设计。

### OQ-004 临时 workspace

**推荐**：空的 app-owned 临时目录，以隔离项目规则和文件。

需要确认：Translation 是否需要读取 AssetIWeave 项目上下文？本阶段默认不需要。

### OQ-005 Registry persistence

**推荐**：Phase 1 使用代码内置定义，不做数据库迁移；出现用户自定义 Agent 管理需求后再设计持久化。

需要确认：首版是否已经需要设置页增删 Agent Definition？

### OQ-006 Feature flag

**Resolved（2026-08-13）**：直接切换且不保留短期回滚开关；真实 OpenCode 验收通过后由 T23 关闭该开放问题。Memory 相关界面和执行链路不在本轮迁移范围。

## 25. 后续演进

Phase 1 通过后，可按独立 Spec 演进：

1. Gemini 和其他 Native CLI 进入 Agent Registry。
2. Agent Definition SQLite persistence 与设置管理。
3. 统一 Agent Event Model。
4. Long-lived connection/session backend。
5. Agent Chat 与 Session persistence。
6. MCP、Skill、Permission UI。
7. Memory 成为 AiExecution consumer。

这些演进不得提前混入 Phase 1。
