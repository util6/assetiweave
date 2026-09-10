# 集成规范：Runtime、Team、Memory 与 Task Center

## 1. 总体数据流

```text
Provider ACP/Native
  → AgentExecutionRuntime
  → AiExecutionProgressSink
  → SessionEventProjection
  → AgentSessionRegistry (AppRuntime)
  → AppService get/subscribe
  → Tauri transport
  → frontend agentSession service
  → AgentSessionProvider/store
  → AgentSessionWorkspace

Memory Job
  → TaskRuntime Memory Stage
  → TaskView.agentSessionRef
  → observer loads the same AgentSessionView
```

## 2. Runtime Registry

### I-RT-001：地址模型

现有 Team key 扩展为内部 typed key：

```text
InternalSessionKey
- tenant_id
- execution_id
- purpose
- optional context discriminator
```

对外返回随机/不可解析 `AgentSessionRef.value`。registry 维护 ref → key/entry 映射；Team adapter 可以继续用 team/member/execution 定位直到 T11。

### I-RT-002：Entry

Entry 至少保存：

- projection；
- metadata（purpose、mode、tenant、agent、model、context）；
- lifecycle state；
- started/updated/finished；
- active/terminal；
- public ref。

### I-RT-003：生命周期

```text
register(pending)
  → attach request item
  → running
  → events
  → terminal
  → retained in bounded registry
  → evicted/unavailable
```

- register 必须在 Agent call 前完成；
- request item 必须在 Provider execute 前应用；
- terminal/error/cancel 必须在所有返回路径 mark terminal；
- cleanup failure 不使已发生的 terminal assistant/tool facts消失；
- AppRuntime shutdown 清空 registry。

### I-RT-004：锁

registry/projection 锁内只做 lookup、clone、state mutation、snapshot materialization。Provider、tool、file、network、DB、Tauri emit 不在锁内执行。

## 3. AppService

### I-AS-001：Get

输入 opaque ref。AppService：

1. 规范化 ref；
2. 读取当前 request tenant；
3. lookup registry；
4. 验证 scope；
5. 由 entry + projection materialize `AgentSessionView`；
6. 返回 typed unavailable 或 view。

前端不直接访问 registry key/HashMap。

### I-AS-002：Subscribe

AppService/runtime 订阅 projection snapshot；Tauri adapter 将其转换为 revision invalidation。订阅结束、receiver lag 或 entry evicted 后，前端通过 get 回填最终事实。

### I-AS-003：Operations

共享 Session View 不新增“任意 send/execute”通用后端入口。Team send/stop/interrupt/replay 保持现有 Team AppService API；Composer adapter 把 capability callbacks 传给共享 UI。Memory observer 没有 operations。

## 4. Tauri Transport

### Commands

- `agent_session_get` 或符合仓库命名的等价命令；
- 参数：typed `AgentSessionGetParams { session_ref }`；
- 返回：`AgentSessionView | AgentSessionUnavailableView`。

### Event

建议 event 名：`agent-session://updated`。

```ts
interface AgentSessionUpdatedEvent {
  sessionRef: AgentSessionRef;
  revision: number;
}
```

- payload 不含 items；
- 前端按 revision 调 get；
- terminal/error/cancel 立即 emit；
- delta 可以合并，但最新 revision 必须可取；
- receiver lag 使用 polling/get 恢复。

若进入 Engine 公开面，registry/surface mapping 与生成 contract 同步；首版仅桌面 UI 需要时可以不暴露 CLI。

## 5. Frontend Service 与 Store

### Service

职责：

- schema parse；
- get Session View；
- subscribe revision event；
- desktop runtime guard；
- 不做 UI 分组。

### Store

按 `sessionRef.value` 保存：

- last valid view；
- requested/loading/error/unavailable；
- local UI state之外的 server revision；
- last event revision；
- retry/backoff metadata。

规则：

- 只接受更高 revision；
- 相同 revision 可用于 idempotent hydration，不覆盖本地展开/scroll；
- 断线保留 last valid view；
- reconnect 立即 get；
- polling 仅在 active/visible Session 或 event unhealthy 时运行；
- terminal Session 降低/停止轮询；
- 页面卸载释放订阅，但 cache 可按有界策略保留。

### Local UI State

以下只属于组件/UI store，不进入 AgentSessionView：

- Thinking/StepGroup/StepDetail expand；
- lane scroll/follow state；
- active Team member；
- Team view mode；
- draft/attachment overlay。

key 使用 sessionRef + item identity，避免流式 snapshot 重置。

## 6. Team 集成

### I-TEAM-001：兼容扩展

T01 增加 Team projection → AgentSessionView adapter，当前页面仍可运行。旧 Team service 不立即删除。

### I-TEAM-002：Session 注册

Team member turn 已有 progress sink 和 runtime registry。迁移时：

- register 返回 public ref；
- Team member projection 携带 ref；
- replay 使用相同 ref/identity 规则；
- terminal mark 保持；
- tenant/team/member scope 仍由 AppService 校验。

### I-TEAM-003：UI 组合

TeamPage/Workspace 负责 roster、view mode、workflow；每个 TeamMemberLane：

1. 取得 member ref；
2. 订阅共享 Session View；
3. 传入 Team capability callbacks；
4. render shared Workspace；
5. 显示 Team typed Plan/Task slots。

### I-TEAM-004：Optimistic Send

- client user item 立即显示；
- request 返回 execution/ref/ack 后绑定真实 identity；
- server snapshot 相同 request 到达后去重；
- failure 保留 draft 和 retry state；
- 切换 lane 不丢其他成员 pending send。

### I-TEAM-005：Workflow Authority

Leader task mode、draft/review/confirm、TeamTask dispatch、pause/interrupt/retry 继续调用已有 Team services。共享组件不直接更改 TeamRun/TeamTask。

## 7. Memory 集成

### 7.1 通用包装

所有 Memory Agent call 使用一个共享 helper/adapter 完成：

1. 根据 job/task/purpose 注册 Agent Session entry；
2. 创建实际 request item；
3. 把 projection sink 组合进原有 Task progress sink；
4. 将 ref 写入当前 Task 的 Agent Stage projection；
5. 执行现有 `execute_agent`；
6. 处理 terminal/error/cancel；
7. 返回原有 `AiExecutionResult` 给 validation/publish。

避免在四个 Memory 文件中复制 registry 与 event wiring。

### 7.2 Session Memory

当前 `AiExecutionRequest.progress=None`。迁移后：

- build prompt 后、execute 前注册；
- request text 等于实际 prompt；
- `AiExecutionPurpose::SessionMemory`；
- context 关联 Memory job ID + Task ID；
- Agent terminal 后继续原有 structured validation/admission/persist；
- observer 失败不影响 Memory Job 执行。

### 7.3 Project Memory

同一 contract，scope=project。项目路径只通过既有 display/path policy呈现；不把内部 locator 自动注入 Session metadata。

### 7.4 Global Memory

同一 contract，scope=global。Tenant scope 必须校验；其他 tenant 不可通过 ref 读取。

### 7.5 Recall

Recall 可能使用 persistent Session。要求：

- 多 Turn 使用稳定 persistent context；
- 每次 execution/turn identity 唯一；
- observer 对已有 Turn 按真实 replay/live 合并；
- Recall 的 reader tools 作为 Tool Steps 展示，但内部 DB path/credential 继续 redacted；
- Recall 产品交互若已有独立聊天入口，不由本规格重做；这里只接入共享 View。

## 8. Task Center 集成

### I-TASK-001：两层详情

- Task View：scope、attempt、领取/加载/Agent/校验/发布、metrics、safe error；
- Agent Session observer：本次 request、assistant、thinking、tools、terminal/error。

两层通过 ref 连接，不复制字段。

### I-TASK-002：Stage Link

Agent Stage 在 register 时获得 ref，Task revision 递增。Stage 尚未 register 时不显示 link。Stage terminal 后 link 在 registry retention 内保持。

### I-TASK-003：Navigation

Task Center 页面保存选中 task。打开 observer 后保留返回信息；返回时恢复同一 task 与 Stage 展开状态。切换 task 关闭/切换 observer，但不取消 Session。

### I-TASK-004：Operations

Task cancel/retry/clear 继续通过 #33 Task APIs：

- cancel 影响 Memory Job cancellation token，并由 Agent runtime发 cancel event；
- retry 创建领域定义的新 attempt/ref；
- clear 只清 Task View terminal record，不主动删除 registry entry；registry 自己有界淘汰。

## 9. Failure Matrix

| Failure | Task View | Observer | Domain effect |
|---|---|---|---|
| Provider spawn/init failed | Agent Stage failed | request + error（若已注册） | Memory 按既有 retry |
| Event receiver lag | 继续阶段 | get 最新 snapshot | 无 |
| Session ref unavailable | 阶段仍可见 | unavailable | 无 |
| Tool content malformed | Agent Stage继续 | unknown bounded block + notice | 不使 execution 自动失败 |
| Session bounds truncation | 阶段无 transcript | truncated 标识 | 无 |
| Observer render error | Task 正常 | error boundary + retry view | 无 |
| Memory validation failed | Validate Stage failed | Agent terminal 仍可看 | Job 按既有语义失败/重试 |
| Publish failed | Publish Stage failed | Agent terminal 仍可看 | Job 保持原子性 |
| User cancels Task | cancelling/canceled | cancel/terminal | 通过已有 token |
| App restart | 恢复 Job成为新 task | 旧 ref unavailable，新执行新 ref | 按 #20 恢复 |

## 10. 数据与持久化

不新增 migration。以下内容不得写入新持久层：

- AgentSessionRef 映射；
- Session items/events；
- Memory prompt 的 viewer 副本；
- Thinking/tool raw input/output；
- UI 展开/滚动状态。

Memory 自身合法产物与 Provider-owned persistent Session 按原合同处理，不因 viewer 改变。

## 11. 性能

- projection apply 的复杂度保持对单 item 局部 materialize；
- Tauri event 只发 ref + revision；
- 前端按 sessionRef selector 更新，Team 一个 lane 的 delta 不重渲染全部 lanes；
- Markdown/Diff/JSON 格式化仅对可见或展开内容执行；
- observer 隐藏后停止 active polling；
- registry 与 per-session bounds 同时生效；
- 大量 delta 测试不得出现 O(events²) 全量重复排序/解析热点。

## 12. #33 基线协调

Issue #33 的 Task Center、TaskRuntime、Memory Stage、Task View DTO 与前端 Provider 基线已由提交 `dcb0fcbf` 落地。执行本专项交叉卡前：

1. 记录 `git status --short`；
2. 读取 #33 最新 Issue/comment/执行包；
3. 读取当前 diff，不恢复旧文件；
4. 以最新 Task View/Memory Stage API 做 additive ref；
5. 验证 `dcb0fcbf` 之后是否存在新的 #33 API 漂移；存在漂移时先按当前代码更新接缝映射，不恢复旧设计；
6. 不把 #31 的完整 Session payload塞入 #33 Task View；
7. 合并后同时运行 #31 与 #33 targeted tests。
