# Team 聊天工作台：代码接缝地图

本地图记录 Issue #21 应复用的当前接缝。执行卡通过 ID 选择上下文；路径变化时只更新本文件。

## 生产接缝

| ID | 当前入口 | 本阶段用法 |
|---|---|---|
| S01 | `src-tauri/src/backend/application/team.rs`、`team_workflow.rs` | AppService Team Authority、成员能力校验、draft/review/confirm、restore 和 execution 调度 |
| S02 | `src-tauri/src/backend/models/team.rs`、`store/team_repo.rs`、Team migrations | Team/TeamMember/TeamRun/TeamTask/mailbox 事实；任务卡投影从这里重建 |
| S03 | `src-tauri/src/backend/ai_execution/types.rs`、`executor.rs`、`bindings.rs` | 通用 Session Event sink、Persistent binding、Resume/Replay 校验和 OneShot 回归边界 |
| S04 | `src-tauri/src/backend/ai_execution/backends/acp.rs`、`acp_aggregator.rs` | ACP live event 翻译、final-text 兼容和 replay 输出 |
| S05 | `src-tauri/src/backend/agents/protocol/acp.rs` | ACP typed SessionUpdate 输入、actor 生命周期、权限、timeout、cancel 和 shutdown |
| S06 | `src-tauri/src/backend/ai_execution/backends/native.rs` | 当前 Native process、resume args、synthetic ID 和 Antigravity model discovery；T04 必须在此层下沉专属 Direct-CLI Adapter，不能在 Team 分支 |
| S07 | `src-tauri/src/backend/agents/types.rs`、`agent_market/types.rs`、`catalog.rs`、`runtime.rs`、`builtin-assets/agent-market/catalog-v1.json` | 语义 Session capability 的声明、安装持久化、reload 和准入依据 |
| S08 | `src-tauri/src/backend/runtime/tasks.rs`、`app_runtime.rs` | 后台 turn/replay task、取消、活动投影、有界缓存与关闭报告 |
| S09 | `src-tauri/src/adapters/tauri/commands.rs`、`background_tasks.rs` | 薄 command、桌面事件和 snapshot/polling fallback |
| S10 | `src-tauri/src/adapters/engine/registry.rs`、`surface_mapping.rs`、protocol/transport | Engine 方法、风险和 Tauri/Engine 表面一致性；公开变化后生成 contract |
| S11 | `cli/cmd/team.go`、`cli/internal/client/engine.go`、`cli/internal/schema/` | Go CLI 只调用 Engine；新状态变化/读取保持可编程 |
| S12 | `frontend/src/services/team.ts`、`teamWorkflow.ts`、`types/team.ts`、`schemas/teamWorkflow.ts` | 前端唯一 backend boundary；新增 member turn、event snapshot、replay 和 restore schema |
| S13 | `frontend/src/app/backgroundTasks/BackgroundTaskRuntime.tsx`、`TeamTaskProvider.tsx`、`AppProviders.tsx` | 事件订阅、轮询、任务合并、全局活动和 Session store 装配 |
| S14 | `frontend/src/pages/team/TeamPage.tsx`、`TeamPage.test.tsx` | 当前 form-first 页面及页面级交互接缝；原地深挖，不创建 `v2` 页面 |
| S15 | `frontend/src/components/foundation/`、`components/common/`、`router/AppRouter.tsx`、`routeLoaders.ts` | chat shell、semantic tokens、responsive 容器、全局 progress 和路由 |
| S16 | `src-tauri/src/backend/conversations/`、Conversation migrations/tests | 只作为零写入和依赖方向回归边界，不作为 Team history 实现入口 |
| S17 | `builtin-assets/adapters/antigravity/adapter.mjs` 与 adapter fixtures/tests | Provider transcript 格式和发现规则的只读证据；Team runtime 不调用 Conversation Adapter |
| S18 | `src-tauri/src/backend/operation_log.rs`、`logs.rs` 与 task snapshots | 正文、tool payload、credential、Resume Anchor 的泄漏回归边界 |

## 推荐新增的深模块

名称可按现有模块组织调整，但所有权必须保持：

- **Session Event**：属于 `ai_execution`，包含通用事件、稳定身份、replay/live 标记与 transient projection。
- **Session Adapter**：属于 Agent Execution/Provider 基础设施，解析 ACP 或 Direct-CLI 原生事件并持有 Resume/History 能力。
- **Antigravity Provider reader**：属于 Provider 基础设施，只读 Provider store；与 Conversation 共享 fixtures/行为契约，不依赖 Conversation application 模块。
- **Team member turn workflow**：属于 AppService Team workflow，校验 member/context 并启动后台 Agent Execution。
- **Frontend Team session store**：置于 app/provider 或 Team hooks 下，通过 frontend services 订阅；不成为持久化层。
- **Chat UI**：在现有 Team 垂直切片内拆成小组件；继续使用 foundation/common 控件。

## 最高层测试接缝

| ID | 接缝 | 必须证明的外部行为 |
|---|---|---|
| TS01 | AppService + 临时 SQLite + 注入 Fake Session Adapter/AgentExecutionRuntime | 独立成员 Session、后台 turn、binding、replay/live、任务投影、恢复状态、Conversation 零写入 |
| TS02 | 通用 Session Event projection/reducer | 稳定 identity、delta/snapshot、tool attachment、sequence、dedup、replay/live merge、bounded eviction |
| TS03 | fake ACP protocol/process | ACP text/thought/tool/live/replay 翻译、权限与 final-text 兼容、cleanup |
| TS04 | fake `agy` executable + transcript fixtures | 真实 ID 捕获、`--conversation` resume、每轮进程、stream-json、完整/降级 history、空 ID 和损坏输入 |
| TS05 | Engine registry + generated contract + Go client | Desktop/Engine/CLI 方法、DTO、错误和风险语义一致 |
| TS06 | frontend services + rendered Team workspace + controllable event source | Leader 默认、头像切换、recipient、stream merge、task mode、task projection、jump、restore states |
| TS07 | TaskRuntime/event subscription + snapshot fallback | 页面离开继续、event miss 恢复、取消、bounded cache、关闭报告和无全局锁阻塞 |
| TS08 | 临时 DB 表计数 + captured logs/snapshots | Team/Conversation 无正文副本，日志与 durable snapshot 无敏感内容 |

TS01 是跨领域主证据；TS06 是用户体验主证据。TS02–TS05、TS07–TS08 只证明所属基础设施契约，不能代替 TS01/TS06。

## 参考项目读取边界

- GoLutra：`~/fork-code/golutra/src/features/chat/`。只读取聊天壳、成员头像、固定输入和滚动行为。
- AionUI：`~/fork-code/AionUi/` 中 Team/AcpChat 相关组件。只读取 typed activity rendering 与成员 Session 切换。
- AionCore：`~/fork-code/AionCore/crates/aionui-session/src/backend/antigravity/`。只读取真实 Conversation ID、每轮进程、stream-json 翻译和 resume 证据。
- AionCore 自有 message persistence 不是实现参考；AssetIWeave 继续使用 Provider history + Team structured facts。

参考代码只提供行为证据。实现必须服从 AssetIWeave 的 AppService、Engine、Agent Execution、TaskRuntime、frontend service 和 Conversation 隔离边界。
