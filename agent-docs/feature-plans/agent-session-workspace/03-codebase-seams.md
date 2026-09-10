# 代码库接缝与参考源

> 本文记录 2026-09-10 的当前事实。执行卡开始时必须重新定位符号，文件路径稳定但行号不作为合同。

## 1. 当前事实摘要

- Rust 已存在协议中立 `SessionEvent`、`SessionItemSnapshot`、`SessionSnapshot` 与 `SessionEventProjection`。
- 当前默认限制为 256 items、2048 events、4 MiB，projection 支持 dedupe、排序、broadcast 和 clear。
- runtime 已存在 `SessionStreamRegistry`，但 key 仍绑定 tenant/team/member/execution，属于 Team-oriented 地址模型。
- Team member execution 已把 progress sink 接到 Session projection；Memory 的 Session/Project/Global `AiExecutionRequest.progress` 当前为 `None`。
- ACP bridge 已产生 assistant/thinking/processing/tool/terminal 事件，但 Tool shape 只有 name/detail，完整 `raw_input/raw_output/content` 尚未成为公开 typed payload。
- 前端 `SessionItemSnapshot` 只有 `text/status/code`；`TeamSessionStore.sanitizeSessionItem` 会把 tool text 清空。
- `TeamWorkspaceShell` 同时承担导航、成员选择、timeline、scroll、composer、Plan/Task 与 item rendering，职责过宽。
- Issue #33 的 Task Center/Task View/Memory Stage 基线已由提交 `dcb0fcbf` 落地；当前 `TaskView`/`TaskStageView` 仍无 agent session ref，本专项只能 additive 扩展该接缝。

## 2. 后端生产接缝

### S-BE-01：Session Event Model

- `src-tauri/src/backend/ai_execution/session_events.rs`
- 修改范围：event/item typed payload、timestamps、truncation、materialization、bounds、Debug redaction。
- 保持：process-local、broadcast、dedupe、monotonic revision、锁外通知。

### S-BE-02：Execution Request 与 Progress Sink

- `src-tauri/src/backend/ai_execution/types.rs`
- 修改范围：公开 request item 的事实投影、progress sink 连接、purpose/mode 元数据。
- 保持：request validation、cancellation、limits、Debug redaction。

### S-BE-03：ACP Bridge

- `src-tauri/src/backend/ai_execution/backends/acp.rs`
- 修改范围：raw input/output、content blocks、tool status、terminal 事件的协议中立映射。
- 保持：ACP 生命周期、permission/security、timeout、cleanup。

### S-BE-04：Native Bridge

- `src-tauri/src/backend/ai_execution/backends/native.rs`
- 修改范围：可得 command/tool/output 映射。
- 保持：只投影 Provider 实际事实，不补齐虚构字段。

### S-BE-05：Runtime Registry

- `src-tauri/src/backend/runtime/session_streams.rs`
- `src-tauri/src/backend/runtime/app_runtime.rs`
- 修改范围：Team-specific key 向 opaque Agent Session ref 扩展；tenant/purpose/context metadata；terminal retention。
- 策略：expand 新地址 API，保留旧 Team adapter，待 T11 收缩。

### S-BE-06：AppService

- `src-tauri/src/backend/application/service.rs`
- `src-tauri/src/backend/application/mod.rs`
- 修改范围：Session View get/subscribe boundary、scope validation、context metadata。
- 保持：AppService 作为唯一业务编排边界。

### S-BE-07：Team Member Workflow

- `src-tauri/src/backend/application/team_member_workflow.rs`
- 修改范围：将现有 Team projection 注册映射到共享 ref，保留 send/replay/cancel 行为。
- 保持：Team role、member scope、persistent binding、task semantics。

### S-BE-08：Memory Producers

- `src-tauri/src/backend/application/session_memory.rs`
- `src-tauri/src/backend/application/project_memory.rs`
- `src-tauri/src/backend/application/global_memory.rs`
- `src-tauri/src/backend/application/memory_recall_workflow.rs`
- 修改范围：为 Agent call 注册 projection、投影 request、设置 progress sink、关联 Task Stage ref、mark terminal。
- 保持：prompt 生成、结构化 validation、Job persistence、atomic publish、retry/lease。

### S-BE-09：Task View

- `src-tauri/src/backend/dto/task_view.rs`
- `src-tauri/src/backend/application/tasks_public.rs`
- Issue #33 的 TaskRuntime/Memory stage projection files
- 修改范围：optional typed ref 与 projection mapping。
- 保持：Task detail 脱敏、TaskRuntime bounds/retention、通知不携带 transcript。

### S-BE-10：Tauri/Engine Transport

- `src-tauri/src/adapters/tauri/commands.rs`
- `src-tauri/src/adapters/tauri/background_tasks.rs`
- `src-tauri/src/adapters/engine/registry.rs`
- `src-tauri/src/adapters/engine/surface_mapping.rs`
- 修改范围：Session get + revision invalidation event；需要时生成 Engine contract。
- 保持：transport only，不读取 runtime HashMap 拼业务 DTO。

## 3. 前端生产接缝

### S-FE-01：现有 Team 类型与 Schema

- `frontend/src/types/team.ts`
- `frontend/src/schemas/team.ts`
- `frontend/src/schemas/teamWorkflow.ts`
- 策略：expand 共享 Agent Session types/schema；Team adapter 逐步迁移；T11 删除重复 shape。

### S-FE-02：Team Session State

- `frontend/src/app/backgroundTasks/TeamSessionStore.ts`
- `frontend/src/app/backgroundTasks/TeamSessionProvider.tsx`
- 当前相反行为：tool text sanitizer 清空详情。
- 策略：先通过 compatibility adapter 输出共享 View，再迁移到共享 store/provider；最终收缩 sanitizer 与重复 merge。

### S-FE-03：Team Workspace

- `frontend/src/components/team/TeamWorkspaceShell.tsx`
- `frontend/src/components/team/TeamPlanCard.tsx`
- `frontend/src/components/team/TeamTaskCard.tsx`
- 策略：页面保留编排，视觉和交互拆成 shared session + Team composition。

### S-FE-04：Team Route

- `frontend/src/pages/team/TeamPage.tsx`
- 策略：保留 load/dialog/workflow orchestration；移除 populated workspace 的营销式 header/nested cards。

### S-FE-05：Frontend Services

- `frontend/src/services/team.ts`
- `frontend/src/services/teamWorkflow.ts`
- 新增共享 Agent Session service；Task Center 通过自身 service 获取 ref，再交给 Session service。

### S-FE-06：Task Center Frontend

- Issue #33 实施后的 Task Center service/provider/page/component。
- 策略：Stage action 只传 ref 打开 observer；Task Center cache 不存 Session items。

### S-FE-07：Theme/Foundation/Common

- `frontend/src/components/foundation/`
- `frontend/src/components/common/`
- `frontend/src/components/ui/`
- 语义 Theme Tokens 所在现有主题文件。
- 策略：通用原子进入 Foundation/Common，Session/Team 业务组合各自内聚；无 raw colors。

## 4. 推荐目标模块边界

目标命名允许按仓库惯例调整，职责不可改变：

```text
frontend/src/
  components/
    agent-session/
      AgentSessionWorkspace
      AgentSessionHeader
      AgentSessionTimeline
      AgentSessionTurn
      AgentSessionMessage
      AgentSessionThinking
      AgentSessionStepGroup
      AgentSessionStepDetail
      AgentSessionComposer
      agentSessionReducer
      useAgentSessionAutoScroll
    team/
      TeamNavigationRail
      TeamWorkspaceHeader
      TeamMemberTabs
      TeamMemberLane
      TeamPlanCard
      TeamTaskCard
    task-center/
      MemoryAgentSessionLink / Observer host
  app/backgroundTasks/
    AgentSessionProvider / store
  services/
    agentSession
  schemas/
    agentSession
  types/
    agentSession
```

组件超过约 200 行时继续按职责拆分；Reducer/normalizer 必须是纯模块，不能埋在 React 组件中。

## 5. 测试接缝

### S-TEST-01：主端到端行为缝隙

Fake Agent → AppService Agent Session View → Tauri/service fixture → shared Provider → AgentSessionWorkspace。

这一个主缝隙覆盖 Team 与 Memory 两种模式，证明不是两套渲染器。

### S-TEST-02：Provider 映射单元缝隙

ACP/Native notification fixture → protocol-neutral SessionEvent。只断言字段映射、identity 与 Debug redaction。

### S-TEST-03：Runtime 行为缝隙

Session registry/projection fixture。断言 tenant scope、dedupe、order、terminal monotonicity、bounds、eviction、unavailable。

### S-TEST-04：Team 页面缝隙

Mock frontend services + shared AgentSessionProvider → TeamPage/workspace。断言 lanes、composer recipient、workflow actions。

### S-TEST-05：Memory/Task Center 缝隙

Fake Memory Agent + Task View → Agent Stage ref → observer。断言 transcript 不在 Task detail，observer 完整且只读。

## 6. AionUi 参考源

固定 commit：`18022a49684d5a2b54b0a47f904e76b17f758b3b`。

### Shell 与组合

- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/components/ChatConversation.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/components/ChatLayout/index.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/components/ChatLayout/chat-layout.css`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/platforms/acp/AcpChat.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/platforms/aionrs/AionrsChat.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/team/components/TeamChatView.tsx`

### Message 与 Steps

- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/MessageList.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/messages.css`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/components/MessageText.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/components/MessageThinking.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/components/MessageToolCall.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/components/MessageToolGroup.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/components/MessageToolGroupSummary.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/acp/MessageAcpToolCall.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/acp/MessageAcpTerminalOutput.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/MessageFileChanges.tsx`
- `~/fork-code/AionUi/packages/desktop/src/common/chat/normalizeToolCall.ts`

### Composer、滚动与导航

- `~/fork-code/AionUi/packages/desktop/src/renderer/components/chat/SendBox/index.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/components/chat/SendBox/sendbox.css`
- `~/fork-code/AionUi/packages/desktop/src/renderer/components/chat/ThoughtDisplay.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/components/chat/CollapsibleContent.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/PlanBar/ConversationPlanBar.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/useAutoScroll.ts`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/Messages/anchorRail/MessageAnchorRail.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/conversation/components/ConversationTitleMinimap/index.tsx`

### Team

- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/team/TeamPage.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/team/components/TeamTabs.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/pages/team/components/TeamViewToggle.tsx`
- `~/fork-code/AionUi/packages/desktop/src/renderer/components/layout/Sider/TeamSiderSection.tsx`

## 7. 参考翻译规则

| AionUi 概念 | AssetIWeave 实现 |
|---|---|
| Arco Button/Input/Card | 现有 Foundation/UI primitives |
| IconPark | 当前 lucide/icon abstraction |
| UnoCSS/raw Aion token | semantic Theme Tokens + repo styling convention |
| ipcBridge | frontend service → Tauri → AppService |
| SWR conversation cache | shared Agent Session Provider/store |
| Aion Conversation DB | 不引入；使用 transient Session View/Provider history |
| TeamChatView reuse | TeamMemberLane compose AgentSessionWorkspace |
| MessageToolGroupSummary | shared StepGroup/StepDetail |
| Aion layout shell | 当前 AppLayout 内的 route workspace |

## 8. Legacy 收缩清单

T11 前保留，T11 后应满足：

- TeamWorkspaceShell 不再定义 SessionItem renderer、Tool 展开和 timeline reducer；
- TeamSessionStore 不再清空 tool text/detail；
- Team-specific Session DTO 不再与共享 DTO 重复维护同一字段；
- Team 页面不再拥有另一个 auto-scroll 实现；
- Memory 页面不出现第二个 transcript renderer；
- Task Center Provider 不缓存 Session items；
- 旧 API 在所有调用点迁移并通过 dead-code/rg 审计后再删除。
