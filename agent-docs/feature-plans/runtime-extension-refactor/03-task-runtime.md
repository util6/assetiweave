# SPEC-03:统一后台 TaskRuntime 与进度通道(P1)

- 状态:Draft v3(v1 审计 #2/#3;v2 复审 #9 修订)
- 前置:SPEC-01(TaskRuntime 挂载于 AppRuntime)
- 进程模型假设(SPEC-00 §3a):**跨调用任务能力仅存在于 ResidentHost(Tauri)**;OneShot Engine 进程内 TaskRuntime 仅用于单请求内的派生并发,CLI 长操作保持前台同步(现状)。
- 交付物:`backend/runtime/tasks.rs`、Tauri `BackgroundTaskRegistry` 适配

---

## 1. 目标

把"后台长任务"的登记、去重、进度、取消、关闭保护从 Tauri adapter 层(`adapters/tauri/background_tasks.rs` 的 `BackgroundTaskRegistry` 与十余种 `*TaskSnapshot`)下沉为 backend 统一机制,并为 SPEC-04 的 outbox dispatcher 提供宿主。

### 非目标

- 不改变现有各任务的业务逻辑与快照字段;不引入持久化任务队列(任务仍是进程内的);不做任务优先级调度。
- **不新增 Engine 契约的 `task.*` 方法**(修订,审计 #2):Engine 是一次性进程,每次 CLI Call 起新进程、单请求即退出——跨调用的任务查询/取消对它没有意义。CLI 后台任务属 daemon RFC(SPEC-00 非目标 9)。任务查询/取消仅经 Tauri command surface 暴露。

## 2. 现状

- `adapters/tauri/background_tasks.rs` 持有任务注册表与每类任务的快照结构(`ConversationSyncTaskSnapshot`、`ConversationSearchIndexTaskSnapshot`、`ConversationScriptInstallTaskSnapshot`、`AiExecutionTaskSnapshot` 等),仅 Tauri 可见。
- CLI 长操作(scan、sync 等)在一次性 Engine 进程内前台同步执行——这是现状也是本轮的既定行为,MUST NOT 顺手改变。
- `AGENTS.md` 已规定:长任务须后台化、快速返回快照、事件+轮询双通道、关闭前检查运行中任务。本 SPEC 是该规定的机制化,不是新规则。

## 3. 设计

`backend/runtime/tasks.rs`:

```rust
pub(crate) struct TaskRuntime { /* 挂在 AppRuntime 上 */ }

#[derive(Clone, Serialize, JsonSchema)]
pub(crate) struct TaskSnapshot {
    pub task_id: String,          // "task-" + ulid
    pub kind: TaskKind,           // 枚举:ConversationSync | SearchIndexRebuild | ScriptInstall | AiExecution | Scan | Backup | BatchMount | ...
    pub dedup_key: Option<String>,
    pub state: TaskState,         // Pending | Running | Succeeded | Failed | Canceled
    pub progress: Option<TaskProgress>,   // { current, total, note }
    pub error: Option<AppErrorView>,      // SPEC-01 AppError 的序列化视图
    pub started_at: String, pub finished_at: Option<String>,
    /// 领域细节:各任务类型自带的扩展字段(沿用现有 *TaskSnapshot 的字段,serde flatten)
    pub detail: serde_json::Value,
}

impl TaskRuntime {
    /// 去重启动:同 (kind, dedup_key) 已有活动任务 → 返回其快照(started=false)。
    pub(crate) fn spawn(&self, spec: TaskSpec, f: impl TaskFn) -> SpawnOutcome;
    pub(crate) fn get(&self, task_id: &str) -> Option<TaskSnapshot>;
    pub(crate) fn list(&self, filter: TaskFilter) -> Vec<TaskSnapshot>;
    pub(crate) fn cancel(&self, task_id: &str) -> CancelOutcome;   // 协作式:置 token,不强杀
}

pub(crate) trait ProgressSink: Send + Sync {
    fn progress(&self, current: u64, total: Option<u64>, note: Option<&str>);
}
```

约束:

1. 任务体 `TaskFn` 在 AppRuntime 的 tokio Runtime 上以 `spawn_blocking` 或 async task 运行;**MUST 使用独立数据库连接或短事务,MUST NOT 持有任何 SPEC-01 §5 的锁跨越阻塞 IO**。
2. 进度属于 **progress event**(可丢失):`ProgressSink` 的默认实现只更新内存快照;Tauri 视图层在其上再桥接 `Emitter` 事件,前端保持"订阅 + 轮询兜底"现状。MUST NOT 把进度写进 SPEC-04 的 outbox。
3. `dedup_key` 约定:`ConversationSync` 用 `source_id` 集合哈希;`Scan` 用 `source_id`;`ScriptInstall` 用 `package_id@version`。与 SPEC-01 §5"扫描去重"条目对应。

## 4. 关闭路径(修订,审计 #3:分阶段,dispatcher 先于 TaskRuntime drain)

由 `AppRuntime::shutdown(grace)` 编排,顺序固定;覆盖 AGENTS.md 的退出保护要求,与 `AppState.allow_exit`/退出确认弹窗对接,替换其中分散的任务检查:

```text
1) 停止外部任务准入        spawn 对新任务返回 ShuttingDown
2) dispatcher 末次派发/drain  outbox dispatcher 完成在途批次或到 grace 超时;
                              v1 消费者同步执行、不派生任务(SPEC-07),故本阶段**无
                              "内部任务准入"语义**;未来消费者需派生任务时再引入
                              SpawnOrigin::{External, Internal}(修订,v2 复审 #9)
3) 停止消费者              取消消费者 cancellation token
4) TaskRuntime.shutdown     等待/取消存量任务,报告未完成清单(ShutdownReport)
5) 关闭数据库连接池
```

MUST NOT 先执行 TaskRuntime shutdown 再 drain dispatcher——dispatcher 是 TaskRuntime 常驻任务,颠倒顺序会先杀掉执行者。

## 5. 迁移步骤

1. 落 `TaskRuntime`(纯新增,不接线),单测:去重、取消、分阶段 shutdown、进度快照。
2. 选一个现有任务(建议 `ConversationSearchIndexTask`,面窄)迁到 TaskRuntime,Tauri 侧 `BackgroundTaskRegistry` 对该类任务改为 TaskRuntime 的只读视图;对外 command 快照字段不变(detail 承载原字段)。
3. 其余任务逐类迁移,每类一个 PR;全部迁完后 `BackgroundTaskRegistry` 退化为纯桥接(或删除,若 Tauri 命令可直接查 TaskRuntime)。

## 6. 验收

- 每类已迁移任务:现有前端页面进度展示、取消、退出保护行为与基线一致(手工冒烟清单:同步、索引重建、脚本安装、AI 执行)。
- 测试:`tasks::tests::dedup_returns_existing_snapshot`、`tasks::tests::cancel_is_cooperative`、`tasks::tests::shutdown_phases_run_in_order`(用探针记录五阶段顺序)、`tasks::tests::shutdown_reports_unfinished`、并发压力下 `list` 无死锁。
- CLI 行为不变:`source.scan` 等长操作仍前台同步完成并返回结果(既有 e2e 不改断言通过)。
- 回归:AGENTS.md 要求的"任务运行中仅禁用冲突操作"不回退——迁移不得引入新的全局禁用。
