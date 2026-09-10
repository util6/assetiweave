# 公开契约：Agent Session View 与 Task Link

> 本文定义实现必须满足的序列化形状与状态机。字段命名以 Rust `snake_case` 内部、Tauri/TypeScript `camelCase` wire 约定为准；最终命名可以按仓库生成规则调整，但语义不可漂移。

## 1. 契约版本

- **C-001**：首版 `schemaVersion = 1`。
- **C-002**：新增字段采用向后兼容 optional/default；删除或改变语义需要新版本。
- **C-003**：所有 opaque ref 只用于 AppService API，不允许前端解析。

## 2. Agent Session Ref

```ts
interface AgentSessionRef {
  schemaVersion: 1;
  value: string;       // opaque, process-local
}
```

规则：

- **C-010**：`value` 不编码供前端解析的 tenant/team/member/execution 结构。
- **C-011**：AppService 在当前 request context 校验 tenant scope。
- **C-012**：未知、已淘汰、上次进程或越权 ref 统一返回 typed unavailable/not-found，不泄漏其他租户存在性。
- **C-013**：Task View、Team member projection 可以携带 ref；不得携带 registry key 内部结构。
- **C-014**：backend 使用 UUID v4 或等强度的 128-bit 随机标识生成 `value`；同一 registry entry 生命周期内稳定，entry 淘汰后不复用。

## 3. Agent Session View

```ts
type AgentSessionState =
  | "pending"
  | "running"
  | "cancelling"
  | "succeeded"
  | "failed"
  | "cancelled";

type AgentSessionTerminalState = "succeeded" | "failed" | "cancelled";

interface AgentSessionTerminal {
  state: AgentSessionTerminalState;
  code: string | null;
  message: string | null;
  retryable: boolean;
}

interface AgentSessionUnavailableView {
  schemaVersion: 1;
  sessionRef: AgentSessionRef;
  state: "unavailable";
  reason: "notFoundOrExpired";
}

interface AgentSessionView {
  schemaVersion: 1;
  sessionRef: AgentSessionRef;
  executionId: string;
  purpose: AgentSessionPurpose;
  mode: "oneShot" | "persistent";
  tenantId: string | null;
  agent: {
    id: string;
    displayName: string | null;
    model: string | null;
    protocol: "acp" | "native" | "unknown";
  };
  context: {
    teamId: string | null;
    memberId: string | null;
    memoryScope: "session" | "project" | "global" | "recall" | null;
    memoryJobId: string | null;
    taskId: string | null;
  };
  state: AgentSessionState;
  terminal: AgentSessionTerminal | null;
  capabilities: AgentSessionCapabilities;
  revision: number;
  eventCount: number;
  items: AgentSessionItem[];
  retention: {
    maxItems: number;
    maxEvents: number;
    maxBytes: number;
    truncated: boolean;
    evictedItemCount: number;
    rejectedEventCount: number;
  };
  startedAt: string | null;
  updatedAt: string;
  finishedAt: string | null;
}
```

- **C-020**：`revision` 从 0 单调递增；任何可见字段变化都递增。
- **C-021**：state 只能按下表单调迁移；未列出的迁移一律拒绝且不增加公开 revision。

| Current | Allowed next |
|---|---|
| `pending` | `running`、`failed`、`cancelled` |
| `running` | `cancelling`、`succeeded`、`failed`、`cancelled` |
| `cancelling` | `cancelled`、`failed` |
| `succeeded` / `failed` / `cancelled` | 无 |

- **C-022**：terminal 为 `succeeded | failed | cancelled` 之一；进入后不可回退。
- **C-023**：`updatedAt` 随公开 revision 更新；item timestamp 不可由前端生成以替代后端事实。
- **C-024**：Agent display name 可缺失，UI 回退到 Agent ID；不伪造名称。

## 4. Purpose 与能力

```ts
type AgentSessionPurpose =
  | "sessionMemory"
  | "projectMemory"
  | "globalMemory"
  | "recall"
  | "teamLeaderChat"
  | "teamMemberTurn"
  | "teamDraft"
  | "teamTask"
  | "teamSummary"
  | "other";

interface AgentSessionCapabilities {
  read: true;
  send: boolean;
  stop: boolean;
  retry: boolean;
  queue: boolean;
  interrupt: boolean;
  attach: boolean;
  mention: boolean;
  slashCommand: boolean;
  modelSelect: boolean;
  permissionResponse: boolean;
  copy: boolean;
  openArtifact: boolean;
}
```

- **C-030**：Memory purpose 的 `send/queue/interrupt/attach/mention/slash/modelSelect/permissionResponse` 全为 false。
- **C-031**：Memory cancel/retry 属于 Task View capabilities，不映射为 Session capability。
- **C-032**：Team capability 根据真实 AppService 操作填充；未实现能力为 false。
- **C-033**：UI 以 capability 显隐控件；不得用 purpose 字符串猜测操作。

## 5. Item Identity

```ts
interface AgentSessionItemIdentity {
  turnId: string;
  itemId: string;
}

interface AgentSessionEventIdentity extends AgentSessionItemIdentity {
  eventId: string;
}
```

registry 已按 `sessionRef` 定位，因此 wire item identity 不需要暴露内部 tenant/team/member key。

- **C-040**：`eventId` 在一个 Session 内唯一，重复事件不增加 revision。
- **C-041**：`itemId` 标识逻辑 item，tool start/update/result 必须复用。
- **C-042**：`turnId` 标识一次请求及其后续执行。
- **C-043**：identity 必须来自 execution adapter/runtime，前端不得重新编号。

## 6. Agent Session Item

```ts
type AgentSessionItem =
  | UserRequestItem
  | AssistantTextItem
  | ThinkingItem
  | ProcessingItem
  | ToolStepItem
  | TaskProjectionItem
  | PlanStatusItem
  | NoticeItem
  | TerminalItem
  | ErrorItem;

interface TruncationInfo {
  originalBytes: number;
  retainedBytes: number;
  strategy: "headTail";
}

interface AgentSessionItemBase {
  identity: AgentSessionItemIdentity;
  sequence: number;
  delivery: "replay" | "live";
  occurredAt: string | null;
  updatedAt: string | null;
  state: "pending" | "streaming" | "completed" | "succeeded" | "failed" | "cancelled";
  partial: boolean;
  truncation: TruncationInfo | null;
}
```

- **C-050**：items 按 `sequence ASC, turnId ASC, itemId ASC` 输出。
- **C-051**：UI 分组不能改变该顺序。
- **C-052**：timestamp 缺失时显示未知，不由客户端猜测。
- **C-053**：`partial` 表示 Provider history/adapter 未提供完整事实；`truncation` 表示已知有内容因 bounds 被裁剪，两者不可混用。

## 7. User Request

```ts
interface UserRequestItem extends AgentSessionItemBase {
  kind: "userRequest";
  text: string;
  source: "interactive" | "memoryJob" | "teamWorkflow" | "replay";
  accepted: boolean | null;
}
```

- **C-060**：interactive 显示用户实际发送文本。
- **C-061**：Memory 显示传入 `AiExecutionRequest.prompt` 的实际文本，标注 source=memoryJob。
- **C-062**：request text 仅进入 transient projection，不进入 Task View、logs 或持久化。
- **C-063**：restore-only replay 没有 request text 时可以不存在，不创建空 user bubble。

## 8. Assistant、Thinking、Processing

```ts
interface AssistantTextItem extends AgentSessionItemBase {
  kind: "assistantText";
  text: string;
  format: "markdown" | "plain";
}

interface ThinkingItem extends AgentSessionItemBase {
  kind: "thinking";
  text: string;
}

interface ProcessingItem extends AgentSessionItemBase {
  kind: "processing";
  phase: "started" | "active" | "completed";
  label: string | null;
}
```

- **C-070**：delta 按接受顺序追加；snapshot 替换已有内容。
- **C-071**：相同 sequence 的 replay 与 live 冲突时 live 胜出。
- **C-072**：terminal text 与最后 assistant accumulated text 完全相同时只更新 terminal，不新增重复正文。
- **C-073**：Thinking 只显示 Provider 返回内容；Processing 不生成 Thinking 文本。

## 9. Tool Step

```ts
interface ToolErrorView {
  code: string | null;
  message: string;
  retryable: boolean;
}

interface ToolExitView {
  code: number | null;
  signal: string | null;
}

interface ToolStepItem extends AgentSessionItemBase {
  kind: "toolStep";
  toolCallId: string;
  name: string | null;
  title: string | null;
  providerKind: string | null;
  status: "pending" | "running" | "succeeded" | "failed" | "cancelled";
  summary: string | null;
  input: ToolContentBlock[];
  output: ToolContentBlock[];
  error: ToolErrorView | null;
  exit: ToolExitView | null;
}

type ToolContentBlock =
  | { type: "text"; text: string; language: string | null }
  | { type: "json"; value: unknown; formatted: string }
  | { type: "command"; command: string; cwd: string | null }
  | { type: "terminal"; stdout: string; stderr: string; ansiStripped: true }
  | { type: "diff"; path: string; oldText: string | null; newText: string | null; unifiedDiff: string | null }
  | { type: "location"; path: string; line: number | null; column: number | null }
  | { type: "image"; path: string; mimeType: string | null; alt: string | null }
  | { type: "artifact"; artifactId: string; renderer: string; title: string | null }
  | { type: "unknown"; providerType: string; display: string };
```

- **C-080**：ToolStart 创建 item 并保留 name/title/kind/input。
- **C-081**：ToolUpdate 原位更新 status/summary/content，不追加逻辑行。
- **C-082**：ToolResult 原位设置 terminal status、output/error/exit。
- **C-083**：ACP `raw_input`、`raw_output` 和受支持 content 映射到 typed blocks；JSON 使用稳定两空格格式化；object key 递归按 Unicode 码点升序，array 保留 Provider 顺序。
- **C-084**：Native/Direct adapters 只映射真实可得字段。
- **C-085**：未知 content 进入 bounded unknown block；不丢弃整个 Step。
- **C-086**：ANSI 在进入 terminal block 前清理；UI 不解释控制序列。
- **C-087**：路径显示使用 home 缩写与现有 path policy；内部 locator 不直接作为普通文本泄露。

## 10. Task、Plan、Terminal、Notice 与 Error

```ts
interface TaskProjectionItem extends AgentSessionItemBase {
  kind: "taskProjection";
  taskId: string;
  title: string | null;
  status: "queued" | "running" | "succeeded" | "failed" | "cancelled";
  detail: string | null;
}

interface PlanStatusItem extends AgentSessionItemBase {
  kind: "planStatus";
  planId: string | null;
  status: "drafting" | "awaitingReview" | "confirmed" | "executing" | "terminal";
  detail: string | null;
}

interface TerminalItem extends AgentSessionItemBase {
  kind: "terminal";
  result: "succeeded" | "failed" | "cancelled";
  text: string | null;
}

interface NoticeItem extends AgentSessionItemBase {
  kind: "notice";
  code: string;
  detail: string | null;
}

interface ErrorItem extends AgentSessionItemBase {
  kind: "error";
  code: string;
  message: string | null;
  retryable: boolean;
}
```

- **C-090**：Error 的用户消息经过既有 public error policy；Provider 原始 stderr 不作为页面错误。
- **C-091**：terminal 一旦进入终态不可被旧事件覆盖。
- **C-092**：Task/Plan typed item 不降级为 notice text。

## 11. 合并状态机

事件应用顺序：

1. 校验 ref scope 与 schema；
2. 计算 event dedup key；已见 event 返回 Duplicate，不递增 revision；
3. 校验 item identity；
4. 规范化内容块与 bounds；
5. 按 `(sequence, deliveryPriority, eventId)` 插入 item event set；
6. 重新 materialize 该 item；
7. 应用 state monotonicity；
8. 执行 session bounds eviction；
9. 递增 revision；
10. 锁外广播 snapshot/revision。

`deliveryPriority`: replay=0, live=1。

终态等级：

```text
pending < streaming < completed
pending < streaming < succeeded | failed | cancelled
succeeded / failed / cancelled 互不覆盖
```

若相同 item 收到互相冲突的不同终态，保留最先按 order key 生效的终态并产生 bounded notice `conflicting_terminal_event`；不静默来回切换。

## 12. Bounds 与截断

默认值保持：

- maxItems = 256
- maxEvents = 2048
- maxBytes = 4 MiB
- registry entries = 256

单个 string/content block 的 retained 上限为当前 `AiExecutionLimits.text_bytes` 与 session 剩余预算的较小值。超限采用 UTF-8 安全的 head/tail 保留：先从总 retained budget 中预留截断标记，再将剩余字节按 75%/25% 分配给开头和结尾，分别向下收缩到合法 UTF-8 边界。`TruncationInfo` 至少包含 `originalBytes`、`retainedBytes`、`strategy="headTail"`。

- **C-100**：不再用“拒绝 oversized event 且 UI 无感”作为正常展示语义。
- **C-101**：eviction 优先最旧 terminal item，再考虑旧 active item；active item 被迫淘汰时 session retention.truncated=true。
- **C-102**：registry 容量满时优先淘汰最旧 terminal Session；全部 active 时按最旧 entry 淘汰并使旧 ref unavailable。
- **C-103**：任何淘汰与截断都不写日志正文。

## 13. Task View Link

```ts
interface TaskView {
  // #33 fields unchanged
  agentSessionRef?: AgentSessionRef | null;
}

interface TaskStageView {
  // #33 fields unchanged
  agentSessionRef?: AgentSessionRef | null;
}
```

- **C-110**：优先把 ref 放在实际 Agent Stage；顶层可选镜像仅用于便捷导航，二者存在时必须相同。
- **C-111**：Task snapshot/detail/event/notification 不携带 Session items。
- **C-112**：没有 Agent call 的 Memory task 不提供 ref。
- **C-113**：恢复 Job 重新执行 Agent 时产生新 ref，并随 Task revision 更新。
- **C-114**：Agent Stage 完成后 ref 在 registry retention 内仍可查看。

## 14. API Surface

AppService 最小公开行为：

- `get_agent_session(ref) -> AgentSessionView | AgentSessionUnavailableView`
- `subscribe_agent_session(ref, after_revision) -> revision invalidation/snapshot event`
- Team 已有 send/stop/interrupt API 保持领域方法；共享 UI 通过 callbacks/service adapters 使用，不新增通用任意执行 API。

Tauri 最小 transport：

- get 命令返回 schema-validated view；
- update event 至少包含 `sessionRef` 与 `revision`，前端收到后按 service 获取/合并；
- event payload 不携带完整 transcript，以减少广播复制和泄漏面。

## 15. Debug 与序列化规则

- `Debug` 对 request/assistant/thinking/input/output/error detail 只显示 `<redacted>` 或长度元数据。
- tracing field 不接受上述正文。
- serialization 只用于显式 Session View transport；不存在 generic `serde_json::to_value(event)` 日志路径。
- HTML/script 永远作为 text/markdown 的受控内容处理；不使用 `dangerouslySetInnerHTML` 渲染 Provider 内容。
