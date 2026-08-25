# 集成设计：翻译、后台任务、Tauri 与 Engine (Translation Task API Integration)

| 字段 | 值 |
|---|---|
| 状态 | Implemented |
| 依赖 | `02-architecture-design.md`、`04-acp-process-runtime-design.md` |

## 1. 当前接口基线

当前 Translation 路径：

```text
ConversationContentCards
  -> frontend/src/services/cardTranslation.ts
  -> invoke("translate_conversation_card")
  -> Tauri spawn_blocking
  -> backend/card_translation.rs
  -> execute_structured_text
  -> AiCliRuntime
```

当前公开请求：

```rust
ConversationTranslationRequest {
  provider,
  cli,
  model,
  prompt,
}
```

当前结果：

```rust
OpencodeTranslationResult {
  translated_text,
}
```

当前设置支持：

```text
provider = cli | google | apple
cli      = opencode | gemini
model    = string
```

Phase 1 不应顺带重做整个 Translation provider schema。

## 2. 兼容策略

### 2.1 保留公开业务 DTO

`ConversationTranslationRequest` 与 `OpencodeTranslationResult` Phase 1 保持不变。Application mapping：

```text
provider=cli, cli=opencode
    -> AiExecutionRequest(agent_id=opencode, purpose=translation)
    -> AiExecutionResult.text
    -> translated_text
```

### 2.2 Gemini

```text
provider=cli, cli=gemini
    -> LegacyGeminiTranslationRuntime
```

要求：

- 原 `AiCliRuntime::Gemini` 逻辑重命名/隔离为明确 legacy seam。
- 新 Agent 不得加入该 enum。
- OpenCode 路径不得 fallback 到该 seam。
- 下一阶段 Native Backend 完成后删除 seam。

### 2.3 Google / Apple

保持 reserved-not-implemented，不纳入本 Spec。

## 3. Agent Runtime 生命周期注入

### 3.1 共享 Runtime

需要一个共享实例提供：

- Registry；
- AgentExecutor；
- concurrency semaphore；
- process/runtime config。

建议：

```rust
pub(crate) struct AgentExecutionRuntime {
  pub(crate) executor: Arc<AgentExecutor>,
}
```

### 3.2 AppService 构造

当前 `AppService` 每次 `open_with_db_path` 构造。为测试与 Desktop 共享并发，建议：

```rust
pub(crate) struct AppService {
  db: Database,
  db_path: PathBuf,
  context: RequestContext,
  agent_runtime: Arc<AgentExecutionRuntime>,
}
```

构造器：

```rust
pub(crate) fn open_with_db_path(path: PathBuf) -> AppResult<Self> {
  Self::open_with_db_path_and_runtime(path, AgentExecutionRuntime::new_default()?)
}

pub(crate) fn open_with_db_path_and_runtime(
  path: PathBuf,
  runtime: Arc<AgentExecutionRuntime>,
) -> AppResult<Self>;
```

说明：

- Engine 创建的 AppService 在进程内持有一个 runtime。
- Tauri AppState 持有共享 runtime，并在 Translation background task 中注入。
- 测试注入 fake Registry/backend，不调用真实 OpenCode。
- 不使用不可替换的全局 singleton。

如果实现者发现 AppService 构造改动波及过大，可将 runtime 作为 application method 参数，但必须保持测试可注入和 Desktop 共享 semaphore；不得退回 Vendor free function。

### 3.3 AppState

建议新增：

```rust
pub(crate) struct AppState {
  // existing fields
  pub(crate) agent_runtime: Arc<AgentExecutionRuntime>,
}
```

初始化失败属于 app startup error，因为 builtin Registry definition 无效是编程错误。OpenCode 未安装不导致启动失败，只影响 availability。

## 4. AppService API

### 4.1 共享执行方法

```rust
pub(crate) fn translate_conversation_card(
  &self,
  params: ConversationTranslationRequest,
) -> AppResult<OpencodeTranslationResult>;
```

OpenCode 分支内部可调用一个 blocking bridge：

```rust
self.agent_runtime.execute_blocking(request)
```

或 async application method。选择必须满足：

- 不在已有 Tokio runtime 中嵌套错误的 `block_on`；
- Tauri task 在 async runtime 中直接 await；
- Engine 可同步等待；
- 所有路径共享 executor core。

推荐把 core 定义为 async，同时提供只给 Engine/application sync seam 使用的受测 blocking facade。

### 4.2 Availability

现有：

```text
check_opencode_translation_availability
```

内部迁移为：

```text
AgentRegistry.check_availability(opencode)
```

返回 DTO 仍为：

```rust
OpencodeTranslationAvailability {
  available,
  version,
  error,
}
```

`version` 来自 definition availability probe `--version`。

### 4.3 Connection test

`test_conversation_translation_connection` 必须走真实 AgentExecutor，但 limits 更短：

- total timeout 推荐 30 秒；
- prompt 固定由前端/业务传入现有 “Reply with OK only.”；
- 仍使用空 workspace、空 MCP、no-tool policy；
- 结果内容不必返回，只映射 available/error。

### 4.4 Model list

`list_conversation_translation_models`：

- OpenCode：通过 Registry 的 `model_discovery` command 执行通用 probe。
- Gemini：保留现有 unavailable/manual-input 行为。
- model discovery 是短命一次性命令，不使用 ACP session。
- command/args 仍必须来自 Definition。
- discovery 复用 `host_process` 一次性执行 helper，不走 `ManagedAgentProcess`。

## 5. 桌面端后台任务 API (Desktop Background Task API)

### 5.1 为什么新增 task API

Translation 可能运行 180 秒并执行网络 I/O。Tauri command 直接 await 虽不持有 global lock，但无法在页面卸载后恢复观察，也不进入 app close running task 检查。因此 Desktop 使用 task API。

### 5.2 新 Tauri commands

建议名称：

```text
start_conversation_card_translation
get_ai_execution_task
list_ai_execution_tasks
cancel_ai_execution_task
```

请求/响应：

```rust
#[tauri::command]
async fn start_conversation_card_translation(
  state: State<'_, AppState>,
  app: AppHandle,
  params: ConversationTranslationRequest,
) -> AppResult<AiExecutionTaskSnapshot>;

#[tauri::command]
fn get_ai_execution_task(
  state: State<'_, AppState>,
  params: AiExecutionTaskGetParams,
) -> AppResult<Option<AiExecutionTaskSnapshot>>;

#[tauri::command]
fn list_ai_execution_tasks(
  state: State<'_, AppState>,
) -> AppResult<Vec<AiExecutionTaskSnapshot>>;

#[tauri::command]
fn cancel_ai_execution_task(
  state: State<'_, AppState>,
  params: AiExecutionTaskCancelParams,
) -> AppResult<AiExecutionTaskSnapshot>;
```

### 5.3 Start command

顺序：

1. 只做轻量 request validation。
2. `BackgroundTaskRegistry.begin_ai_execution` 创建 queued snapshot + cancellation token。
3. 立即返回 snapshot。
4. spawn async task。
5. task 注入 shared AgentExecutionRuntime。
6. progress sink 更新 snapshot phase 并 emit event。
7. result/error 后先由 executor cleanup，再 finish task。
8. emit terminal snapshot。

不得持有 `state.lock`。

### 5.4 BackgroundTaskRegistry 扩展

现有中央 registry 增加：

```rust
ai_executions: Mutex<HashMap<String, AiExecutionTaskEntry>>
```

```rust
struct AiExecutionTaskEntry {
  snapshot: AiExecutionTaskSnapshot,
  cancellation: AiExecutionCancellation,
}
```

API：

```rust
begin_ai_execution
update_ai_execution_phase
finish_ai_execution
cancel_ai_execution
ai_execution_snapshot
ai_execution_snapshots
prune_ai_executions
```

不要另建第二个 Tauri 全局 registry。

### 5.5 Snapshot DTO

```rust
#[derive(Clone, Debug, Serialize)]
pub(crate) struct AiExecutionTaskSnapshot {
  pub(crate) id: String,
  pub(crate) purpose: AiExecutionPurpose,
  pub(crate) agent_id: String,
  pub(crate) state: AiExecutionTaskState,
  pub(crate) phase: AiExecutionPhase,
  pub(crate) created_at: String,
  pub(crate) updated_at: String,
  pub(crate) finished_at: Option<String>,
  pub(crate) result: Option<AiExecutionPublicResult>,
  pub(crate) error: Option<AiExecutionErrorView>,
}
```

Translation public result：

```rust
AiExecutionPublicResult {
  text: String,
}
```

Snapshot 禁止包含 prompt、cwd、env、stderr。

### 5.6 Event

```text
AI_EXECUTION_TASK_UPDATED_EVENT = "ai-execution://task-updated"
```

每次 phase/state 变化 emit 完整 snapshot。

事件频率低，无需 token-level emit。Translation UI 不展示流式文本。

### 5.7 Cancel

`cancel_ai_execution_task`：

- task 不存在 -> not found error；
- terminal task -> 幂等返回现有 snapshot；
- queued/running -> set cancellation；
- snapshot 可先变为 running + cancelling phase，最终由 executor cleanup 后变 cancelled；
- 不得在 command 线程直接 kill process。

### 5.8 Retention

- terminal tasks 保留 10 分钟；
- 最多保留 100 条 terminal snapshots；
- running/queued 永不因 retention 淘汰；
- prune 在 begin/list/finish 时 opportunistic 执行即可；
- app restart 后 tasks 清空。

## 6. 现有 Tauri 命令兼容性 (Existing Tauri Command Compatibility)

现有：

```text
translate_conversation_card
translate_conversation_card_with_opencode
```

推荐：

- `translate_conversation_card` 保留给 Engine registry 和短期 Tauri compatibility，但 frontend 不再调用。
- `translate_conversation_card_with_opencode` 标记 deprecated，内部映射到统一 request；没有使用者后删除需单独确认 contract。
- compatibility command 仍必须走 AgentExecutor，不允许继续 `opencode run`。

## 7. Engine Contract

### 7.1 保持同步 method

```text
conversation.card.translation.run
```

保持现有 request/result，Engine 在一次 command 生命周期内等待 execution。

原因：CLI 启动的 Engine stdio process 生命周期与 Desktop 不同，返回内存 task id 后 process 退出会丢失任务。

### 7.2 共享实现

Engine registry 仍调用：

```rust
service.translate_conversation_card(params)
```

AppService 内部使用 AgentExecutionRuntime，不复制 Tauri task code。

### 7.3 Cancel

Phase 1 CLI 不新增跨进程 cancel method。CLI 进程被用户中断时，Engine shutdown/drop 必须触发 executor cancellation 与 process cleanup。需要 e2e test 验证。

### 7.4 Contract regeneration

如果仅新增 Tauri-only task commands，Engine contract 不必暴露它们。若 Rust DTO 影响 Engine method schema，执行：

```bash
pnpm cli:contract
```

并检查 generated diff。

## 8. Frontend Service

### 8.1 Types

在 `frontend/src/services/cardTranslation.ts` 增加：

```ts
export type AiExecutionTaskState =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface AiExecutionTaskSnapshot {
  id: string;
  purpose: "translation" | "connection_test";
  agent_id: string;
  state: AiExecutionTaskState;
  phase: AiExecutionPhase;
  created_at: string;
  updated_at: string;
  finished_at: string | null;
  result: { text: string } | null;
  error: AiExecutionErrorView | null;
}
```

### 8.2 Functions

```ts
startConversationCardTranslation(request)
getAiExecutionTask(taskId)
listAiExecutionTasks()
cancelAiExecutionTask(taskId)
```

原 `translateConversationCardContent` 可改为 compatibility helper：start + observe terminal；页面组件推荐直接使用 provider/hook，不要在 service 内创建无法跨页面恢复的 polling loop。

## 9. Frontend Provider

建议：

```text
frontend/src/app/backgroundTasks/AiExecutionTaskProvider.tsx
```

职责：

- app 启动/list 恢复现有 in-memory tasks；
- listen full snapshot event；
- 1 秒 polling fallback（有 running/queued task 时）；
- merge by id；
- 暴露 start/cancel/get；
- terminal retention UI 可自行只展示最近项。

Context：

```ts
interface AiExecutionTaskContextValue {
  tasks: AiExecutionTaskSnapshot[];
  startTranslation(request): Promise<AiExecutionTaskSnapshot>;
  cancelTask(taskId: string): Promise<AiExecutionTaskSnapshot>;
  getTask(taskId: string): AiExecutionTaskSnapshot | undefined;
}
```

Provider 复用 Memory/SearchIndex/SkillBackup provider 的 event + polling 模式与测试习惯。

## 10. ConversationContentCards Integration

当前组件维护：

```text
translatingBlockIds
translatedBlocks
translationErrors
```

新增：

```text
translationTaskByBlockId: Record<blockId, taskId>
```

### 10.1 Start

1. 点击 block translate。
2. 调用 provider.startTranslation。
3. 保存 block -> task id。
4. 当前 block 显示 phase label 与 cancel action。
5. 不禁用其他 block；受后端 concurrency=2 控制。

### 10.2 Observe success

当 task succeeded：

1. 读取 `result.text`。
2. 若 block 有 partId，调用现有 `translationSaver`。
3. 更新 `translatedBlocks`。
4. 清理 block/task correlation。

保存失败是 Conversation persistence error，不反向把已完成 Agent task 改成 failed；UI 显示保存错误并保留 result 供重试保存。

### 10.3 Observe failure/cancel

- failed：显示 `error.message`。
- cancelled：恢复 translate action，不作为红色错误。
- unmount：不 cancel；task 由 provider/global registry 继续。

### 10.4 Progress copy

建议映射：

| Phase | 用户文本 |
|---|---|
| queued | 等待执行 |
| resolving / spawning | 正在启动 OpenCode |
| initializing / creating_session | 正在连接 Agent |
| configuring | 正在应用模型 |
| prompting | 正在翻译 |
| cancelling | 正在取消 |
| closing / cleaning_up | 正在收尾 |

需要中英文 i18n key，不直接硬编码。

## 11. Global Task Indicator

现有 app close 已检查 `background_tasks.has_running_tasks()`。扩展该方法使 AI task 被计入。

全局 indicator 最小范围：

- running/queued Agent task 数量；
- 最近 task 的 phase；
- 点击可回到 Conversation 页面或展开任务列表；
- 提供 cancel；
- terminal task 可 dismiss。

如果产品评审决定 Phase 1 不做完整列表，至少必须有全局 running count 与 app close protection。

## 12. Connection Test UI

连接测试也可通过 task runtime 执行，但设置对话框希望得到单次 result。两种实现：

- 推荐：调用共享 AppService sync/await command，30 秒内显示 checking；它不进入全局长期 task列表。
- 或统一 task：purpose=connection_test，并由设置页观察。

由于连接测试是显式短操作，Phase 1 推荐前者，前提是不持 global lock，关闭设置页时 cancellation 能传递。

## 13. Model List UI

现有刷新模型按钮继续调用同名 command。后端改为 definition-driven discovery。失败不影响手工输入 model。

Model list output normalization保持：

- trim；
- 忽略空行与 command banner；
- 最多 500 项；
- 去重并保持稳定顺序（建议新增）；
- 不把 stderr 当 model。

## 14. App Close

### 14.1 检测

`BackgroundTaskRegistry::has_running_tasks()` 包含 queued/running AI tasks。

### 14.2 用户确认退出

确认后：

1. 调用 registry cancel all AI tasks；
2. 等待最多 5 秒 cleanup convergence；
3. 超时记录 high-priority diagnostic；
4. 继续现有 shutdown flow。

不得只关闭窗口而让 child process 成为 orphan。

## 15. API Test Cases

### Tauri

- start 立即返回 queued/running snapshot。
- get/list 返回完整 snapshot。
- cancel 幂等。
- task event 为 full snapshot。
- frontend 不再调用同步 translate command。

### Engine

- method 名与 request/result 不变。
- OpenCode route 使用 ACP fake。
- engine process interrupt 清理 child。
- Gemini compatibility test 保持。

### Frontend

- page unmount 后 task 继续并由 global provider 收到 terminal event。
- missed event 后 polling 恢复。
- unrelated buttons enabled。
- success save 与 save failure 分离。

## 16. 集成验收检查表

- [ ] AppState 共享一个 AgentExecutionRuntime。
- [ ] Tauri start 不持 global lock。
- [ ] BackgroundTaskRegistry 是唯一 Desktop task truth。
- [ ] Engine 与 Desktop 共享 executor core。
- [ ] existing Translation DTO 兼容。
- [ ] OpenCode 不走 legacy CLI。
- [ ] Gemini 行为不回归。
- [ ] frontend service 是唯一 invoke boundary。
- [ ] app close 能 cancel 并等待 Agent task cleanup。
