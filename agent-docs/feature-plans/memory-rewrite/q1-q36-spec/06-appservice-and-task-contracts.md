# Memory Q1–Q36：AppService、公开 API 与 Task 合同

## 1. 架构边界

```text
React -> frontend service -> Tauri command -> AppService
CLI   -> Engine stdio     -> AppService
ACP tools/MCP             -> AppService read capabilities
AppService -> repositories/runtime -> SQLite/filesystem
```

- 业务校验、调度、Skill 解析、准入、last-success 和投影触发只实现于 AppService。
- adapter 只做参数/错误/DTO 转换。
- CLI 不读取 SQLite 或 Memory 文件。
- frontend component 不直接调用 `invoke`。
- Worker 在 AppService 内部调用 ACP executor，不启动 CLI/AIWC 子进程。

## 2. Canonical API

新公开 JSON DTO 统一使用 camelCase。Rust 内部模型可使用 snake_case 字段，但序列化合同必须稳定。

### 2.1 `memory.recent.snapshot.get`

输入：空对象。tenant 来自执行上下文，不允许调用方提供。

输出：`RecentMemoryStateView`。读取本身成功时始终返回容器，使“尚无 Snapshot”和“首轮生成失败”可以区分。

```text
RecentMemoryStateView
  status: empty | generating | ready | update_failed
  snapshot: RecentMemorySnapshotView | null
  latestAttemptTaskId: nullable
  latestAttemptError: nullable public error
```

```text
RecentMemorySnapshotView
  snapshotId
  sequence
  targetWatermark
  windowStart
  windowEnd
  windowHours
  publicationKind: generated | reused
  reusedFromSnapshotId: nullable
  contentGeneratedAt
  publishedAt
  projects: RecentProjectView[]
```

```text
RecentProjectView
  projectKey
  projectTitle
  projectPath: nullable
  summary
  noMaterialChange
  latestActivityAt
  sourceSessionCount
  items: RecentMemoryItemView[]
```

```text
RecentMemoryItemView
  itemId
  revisionId
  category
  status
  title
  summary
  rationale
  occurredAt
  recommendationRank: nullable
  sourceAvailability: available | partially_unavailable | unavailable
  sessionReferences: RecentSessionReferenceView[]
```

```text
RecentSessionReferenceView
  sourceId
  sessionId
  sessionTitle
  sourceAgent
  lastActivityAt
  available
  unavailableReason: nullable
```

项目与 Item 按稳定顺序返回，但 DTO 不预先编码“时间视图/项目视图”树；前端对同一数组做两种投影。

没有成功 Snapshot 时 `snapshot=null`。最新 Job 失败但已有 last-success 时仍返回该 Snapshot，并令容器 `status=update_failed`；错误不包含正文、Prompt、locator 或工具参数。

### 2.2 `memory.project.get`

输入：`projectPath`。AppService 先执行 Project Directory 规范化。

输出：当前项目 L2 视图或 null：project identity、current item revisions、source availability、last successful consolidation、revision hash。不得返回旧 Markdown 作为结构化字段。

### 2.3 `memory.context.resolve`

保持现有输入语义：optional project path、query、positive token budget。输出扩展为当前 L2/L3 revision references 与 coverage/truncation；不得等待生成或解析 Markdown。

### 2.4 `memory.rebuild`

维护入口，不出现在 Recent 页。

```text
input
  target: recent | project | global | all
  projectPath: required only for project
  reason: manual | migration | projection_repair

output
  accepted
  scheduledTaskIds
  targetWatermark: nullable
  reused: false at enqueue time
```

- `recent` 使用最新已到期水位，而不是调用瞬间作为随意窗口结束时间。
- `project/global` 绕过低频时间门，但仍遵守候选、引用、Skill、Schema 和权限准入。
- `projection_repair` 不调用 Agent，只从 SQLite last-success 重建文件。

### 2.5 `memory.task.*`

保留 list/get/cancel/retry。Task DTO 的 `detail.domain` 使用：

- `session_memory`
- `recent_snapshot`
- `project_memory`
- `global_memory`
- `memory_projection`
- `memory_recall`

公开进度只包含阶段、数量和安全文案。retry 必须定位同一个 Durable Job；配置已变化导致旧目标 stale 时返回 conflict，并由协调器创建新目标。

## 3. 兼容与退出

### Expand

- 新增 `memory.recent.snapshot.get`，旧 `memory.recent.list` 继续返回当前 72h Session/Event DTO。
- 新 Recent 页面和新 `assetiweave-memory` 版本切换到 Snapshot API。
- Engine contract 同时发布两种方法，旧方法标 deprecated。

### Migrate

- Tauri、frontend service、CLI 和 Skill 全部使用 Snapshot API。
- usage/Recall 若仍读取旧 Recent Event，改读 Memory Item/Session Memory。
- 通过 surface matrix 和仓库搜索证明旧方法没有 active caller。

### Contract

- 从 Engine registry、Tauri command、CLI command/schema 和 Skill capability 中移除 `memory.recent.list` 与旧 event target。
- 旧数据库表可以保留历史审计，但不再有 active read/write dependency。

## 4. Settings 合同

继续使用统一 `get_app_settings`/`save_app_settings`：

- Backend typed settings 与 frontend schema 同时增加 window、水位、generation Skill asset ID。
- AppService 保存前验证枚举、时间格式、两个水位不同和 Skill asset 可见性。
- 保存成功后发布 settings changed 事件并唤醒 Memory coordinator。
- 保存不调用 Agent、不等待 Job、不修改当前 last-success。
- 设置页“打开 Skill”复用 Asset/Source 导航或系统打开能力；“创建可编辑副本”复用 Skill Library 复制能力。需要新增能力时仍以 AppService 为入口。

## 5. Engine、Tauri 与 CLI

### Engine canonical methods

- `memory.recent.snapshot.get`
- `memory.context.resolve`
- `memory.project.get`
- `memory.rebuild`
- `memory.task.list`
- `memory.task.get`
- `memory.task.cancel`
- `memory.task.retry`

深度回忆现有方法保持不变。

### Tauri commands

命名遵循现有 snake_case 适配规范。每个 command 只解析参数、调用 AppService、返回 DTO；不得包含 SQL、路径或调度判断。

### CLI

目标命令：

```bash
aiwc memory recent get
aiwc memory context resolve --current-project --query QUERY --token-budget 2000
aiwc memory project get PROJECT_PATH
aiwc memory rebuild --target recent
aiwc memory rebuild --target project --project PROJECT_PATH
aiwc memory rebuild --target global
aiwc memory task list --active-only
aiwc memory task get TASK_ID
aiwc memory task cancel TASK_ID
aiwc memory task retry TASK_ID
```

默认输出面向人类；`--json` 使用 Engine DTO。CLI 只负责格式化和参数校验，不重新分组、重新计算窗口或读 Markdown。

## 6. TaskRuntime 与 Durable Job

- Durable Job 是恢复 Authority；TaskRuntime 是当前进程活动投影。
- 入队先提交 SQLite，再向 TaskRuntime 注册/调度。
- 进程启动执行 queued/retry/expired lease/recoverable running 扫描。
- ownership token 必须在心跳、提交、失败和取消时验证。
- 取消是幂等请求；Agent 返回的晚到结果在提交前因 token/状态检查被拒绝。
- 同一 tenant 同一 target fingerprint 至多一个 queued/running Job。
- Recent 每 tenant 串行；Project 每 project key 串行、不同项目可并行；Global 每 tenant 串行。
- Agent 调用、文件写入与长查询在全局 app lock 外执行。

## 7. 错误合同

| Code | HTTP/Engine 语义 | retryable |
|---|---|---:|
| `MEMORY_SCHEDULE_INVALID` | validation | 否 |
| `MEMORY_SKILL_NOT_FOUND` | validation/configuration | 否 |
| `MEMORY_SKILL_INVALID` | validation/configuration | 否 |
| `MEMORY_NO_AGENT_ASSIGNMENT` | configuration | 否 |
| `MEMORY_EVIDENCE_INCOMPLETE` | generation failed | 视上游状态 |
| `MEMORY_EVIDENCE_BUDGET_EXHAUSTED` | generation failed | 否，直到输入/策略变化 |
| `MEMORY_OUTPUT_SCHEMA_INVALID` | generation failed | 是，受限重试 |
| `MEMORY_REFERENCE_OUT_OF_SCOPE` | generation failed/security | 否 |
| `MEMORY_RESULT_STALE` | conflict | 否 |
| `MEMORY_PROJECTION_FAILED` | projection failed | 是 |
| `MEMORY_TASK_NOT_RETRYABLE` | conflict | 否 |

公开错误包含 code、message、retryable 和安全 details；内部 cause 可以进入 tracing，但不包含 Prompt 或 Conversation 正文。

## 8. 事务和事件

- Snapshot 事务成功后发布 `memory-recent-snapshot-updated`，payload 只含 tenant、安全 snapshot metadata。
- projection 成功后发布 `memory-projection-updated`。
- task 状态继续使用统一任务事件；frontend 采用事件 + polling fallback。
- 事件丢失不影响 Authority；重新读取 AppService 必须恢复正确状态。
- SQLite last-success pointer 与对应成功版本必须同事务更新。
