# Team 多 Agent：代码接缝地图

本地图记录当前应复用的生产和测试接缝。路径变化时只更新本文件；执行卡引用 Seam ID。

## 生产接缝

| ID | 当前位置 | 复用方式 |
|---|---|---|
| S01 | `src-tauri/src/backend/application/service.rs`、`system.rs` | AppService 和生产/测试 runtime 装配；沿用 `open_with_db_path_and_runtime` 注入 fake runtime |
| S02 | `src-tauri/src/backend/ai_execution/types.rs`、`executor.rs` | 扩展 `AiExecutionRequest`、Persistent 上下文和 `AgentExecutionRuntime`，不创建平行 runtime |
| S03 | `src-tauri/src/backend/ai_execution/backends/acp.rs` | Persistent ACP 执行、cleanup 分流、稳定 workspace 和结果映射 |
| S04 | `src-tauri/src/backend/agents/protocol/acp.rs` | 使用当前 ACP SDK typed request 接入 load/resume/replay；沿用 actor、timeout、cancel、shutdown |
| S05 | `src-tauri/src/backend/ai_execution/backends/native.rs` | Native resume 参数和进程生命周期；Provider 差异来自 Definition |
| S06 | `src-tauri/src/backend/agent_market/types.rs`、`catalog.rs`、`repository.rs`、`runtime.rs` | 声明、持久化和解析 Resume/Replay/Team-tool 能力及 Native 参数 |
| S07 | `src-tauri/src/backend/runtime/tasks.rs`、`app_runtime.rs` | 活动 Team 任务、ResidentHost、关闭报告和 task projection |
| S08 | `src-tauri/src/backend/events/` | transaction outbox、consumer offset、重复投递和常驻 dispatcher |
| S09 | `src-tauri/src/adapters/tauri/commands.rs`、`background_tasks.rs` | 薄 Tauri command、事件、轮询 fallback 和 task snapshot |
| S10 | `src-tauri/src/adapters/engine/registry.rs`、`protocol.rs`、`transport.rs` | Engine 方法注册、DTO 和协议传输；公开变化后生成 contract |
| S11 | `cli/internal/client/engine.go`、`cli/internal/schema/`、`cli/cmd/` | Go CLI 只调用 Engine；provider fallback 也从这里进入 AppService |
| S12 | `frontend/src/services/` | Team 前端唯一后端边界；页面、hooks、schema 不直接 `invoke(...)` |
| S13 | `frontend/src/app/backgroundTasks/`、`AppProviders.tsx` | 事件订阅、轮询恢复、全局状态和 indicator 的既有模式 |
| S14 | `frontend/src/router/`、`pages/`、`components/foundation/`、`components/layout/` | Team route、workspace 页面、渐进加载和共用 foundation 控件 |
| S15 | `src-tauri/src/backend/conversations/`、Conversation migrations/tests | 只用于零写入回归断言，不作为 Team 实现依赖 |

## 建议新增的领域模块

以下是所有权建议，不是必须逐字采用的文件名：

- `backend/team`：Team aggregate、状态机、repository port、coordinator 和 mailbox policy。
- `backend/application/team`：AppService Team workflow。
- `frontend/services/team`、`hooks/team`、`pages/team`、`components/team`：前端 service-first 垂直切片。
- Team persistence 使用新 migration；Provider binding persistence 归 `ai_execution`/runtime infrastructure。

模块命名必须保持 Team 与 Conversation 可搜索地分离。Team 代码只有在做零写入断言或展示跳转时才引用 Conversation。

## 最高层测试接缝

| ID | 接缝 | 必须证明的行为 |
|---|---|---|
| TS01 | AppService + 临时 SQLite + 注入 Fake `AgentExecutionRuntime` | Team 状态机、人工门、固定分配、恢复、Conversation 零写入 |
| TS02 | fake ACP process/protocol | new→persist binding→resume→replay、dead anchor、MCP reinjection、process cleanup |
| TS03 | Agent Market catalog/repository/runtime | 能力和声明从 catalog 到 reload 后 execution 完整保留 |
| TS04 | Engine registry + generated contract + Go client | Desktop/CLI 共享方法、DTO、错误和风险语义 |
| TS05 | frontend service + component interaction | service-only、review gate、任务状态、恢复时间线、事件/轮询 fallback |
| TS06 | AppRuntime/outbox + 临时 DB | restart recovery、重复投递、幂等 dispatch、bounded shutdown |

TS01 是唯一跨领域主接缝。低层测试只证明 Provider、transport、catalog 或 task runtime 自己的契约，不代替 TS01。

## 参考项目读取边界

- AionCore：`~/fork-code/AionCore/crates/aionui-team`、`aionui-session`、`aionui-ai-agent`。
- OpenCode：`~/fork-code/opencode/packages/opencode/src/acp`。
- GoLutra：只读取与当前执行卡直接相关的 Team UI/协作 symbol。

参考代码只提供行为证据。实现必须服从 AssetIWeave 的 AppService、Engine、Extension Kernel、TaskRuntime 和 Conversation 隔离约束，不复制参考项目的 Conversation ownership。
