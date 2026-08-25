# SPEC-BA-02：TaskRuntime 唯一生命周期与长任务迁移

- 状态：Proposed v1
- 优先级：P0/P1
- 前置：SPEC-BA-01 的 typed error 基础可与本篇并行，但最终 DTO 必须使用 AppError view
- 进程裁决：ResidentHost 后台化；OneShot Engine 同步执行同一个业务 workflow

## 1. 当前问题

`BackgroundTaskRegistry` 仍维护 conversation sync、script install、backup、search index、
Memory、AI、Agent lifecycle、Agent Market refresh 八组状态容器。TaskRuntime 已负责去重、
取消和 shutdown，但领域 snapshot 仍手工维护 running/terminal 状态。

此外：

- `scan_sources`、`scan_skill_sources` 是同步 Tauri command。
- group mount 和 exclusive group mount 同步执行批量文件与数据库操作。
- `TaskKind::Scan`、`TaskKind::BatchMount` 没有生产注册路径。

## 2. 核心不变量

1. 同一个 `task_id` 的生命周期状态只能来自 `TaskRuntime`。
2. Projection 删除不会取消任务；TaskRuntime 删除/终止必须立即反映到 Projection 读取结果。
3. Domain progress/result 可以独立存储，但不得包含可独立演进的 lifecycle state。
4. 完成业务结果之前不得把 Task 标成 Succeeded。
5. Task 进入 terminal 后不得恢复 Running。
6. 取消必须是幂等的；未知 task 返回 `not_found`。
7. 关闭等待必须有上界，并报告未完成 task ID。

## 3. 目标数据模型

```rust
pub(crate) struct DomainTaskProjection<P, R> {
    pub task_id: String,
    pub progress: P,
    pub result: Option<R>,
    pub domain_error: Option<WireError>,
}

pub(crate) struct DomainTaskSnapshot<P, R> {
    #[serde(flatten)]
    pub lifecycle: TaskSnapshot,
    pub progress: P,
    pub result: Option<R>,
    pub domain_error: Option<WireError>,
}
```

读取流程：

```text
TaskRuntime.get(task_id)
  + DomainProjectionStore.get(task_id)
  → assemble DomainTaskSnapshot
```

若 TaskRuntime 不存在该 ID：

- 查询单个 task 返回 `not_found`。
- list 不得返回 Projection 中残留的 Running。
- terminal projection 可按 retention policy 保留，但 lifecycle 必须来自保留的
  TaskRuntime terminal snapshot；不能只保留 domain map。

## 4. BackgroundTaskRegistry 收口

### 4.1 允许职责

- 领域 DTO 组装。
- 领域进度/result 存取。
- Tauri event emit。
- 调用 TaskRuntime register/start/cancel/complete。

### 4.2 禁止职责

- 自己判断“是否有运行中任务”。
- 自己决定 queued/running/succeeded/failed/cancelled。
- 用本地 map 决定 dedup/conflict。
- 单独清理 terminal lifecycle。
- 在 TaskRuntime 已取消后继续更新为 Running。

### 4.3 迁移完成形态

`BackgroundTaskRegistry` SHOULD 重命名为 `TaskProjectionRegistry`。若为兼容保留旧名，旧名
只能是 type alias，并必须登记删除版本。

## 5. Source Scan 后台化

### 5.1 共享 workflow

```rust
pub(crate) struct SourceScanWorkflow;

impl SourceScanWorkflow {
    pub fn run(
        service: &AppService,
        params: SourceScanParams,
        cx: &TaskContext,
    ) -> AppResult<SourceScanResult>;
}
```

`run` 必须：

1. 读取和去重待扫描 source。
2. 每个 source 前检查 cancellation。
3. 扫描时不持有 AppRuntime 全局锁。
4. 批量收集结果；按当前事务边界持久化。
5. 每个 source 更新一次有界 progress。
6. 全部完成后只触发一次必要的 catalog/status refresh。

### 5.2 ResidentHost surface

新增或确认以下 Tauri-only command：

```text
start_source_scan(params) -> SourceScanTaskSnapshot
get_source_scan_task(taskId) -> SourceScanTaskSnapshot
list_source_scan_tasks() -> Vec<SourceScanTaskSnapshot>
cancel_source_scan(taskId) -> SourceScanTaskSnapshot
```

`start_source_scan` 必须在目标机器正常规模数据下快速返回，不等待实际扫描完成。

### 5.3 OneShot surface

Engine 现有 `scan_sources` 保持同步，但必须直接调用 `SourceScanWorkflow::run`，不得复制扫描
或持久化逻辑。OneShot 创建请求内 `TaskContext`，取消来源仅限进程信号/请求 deadline。

## 6. Batch Mount/Unmount 后台化

### 6.1 统一输入

```rust
pub(crate) enum BatchMountMode {
    Group { group_id: String, enabled: bool },
    Exclusive { profile_id: String, group_ids: Vec<String> },
    Explicit { profile_id: String, asset_ids: Vec<String>, enabled: bool },
}
```

所有输入 MUST：

- 去除空 ID。
- 稳定去重。
- 一次加载共享 Profile、assets 和 mounts。
- 先生成 preview，再执行。
- 每个物理变更前检查 cancellation。
- 对同一 profile 使用 conflict key `mount-profile:{tenant}:{profile}`。

### 6.2 原子性与部分失败

文件系统和 SQLite 无法形成单一事务，因此采用明确的补偿模型：

1. preview 记录计划动作。
2. 单项执行物理挂载。
3. 物理成功后原子写入 mount intent + observation。
4. DB 写失败时尝试恢复该单项物理状态。
5. 失败记录到 result；默认继续下一项，除非 safety conflict。
6. Task result 必须区分 succeeded、partial_failure、failed、cancelled。

TaskRuntime lifecycle：

- 无 item error → `Succeeded`。
- 有 item error 但 workflow 完成 → `Succeeded`，领域 result.status 为
  `partial_failure`；不得伪装为系统错误。
- workflow/安全不变量失败 → `Failed`。
- cancellation 生效 → `Canceled`。

## 7. Progress 与事件

Progress 不是 Domain Event，不写 outbox。

```rust
pub(crate) struct BatchProgress {
    pub phase: BatchPhase,
    pub completed: u64,
    pub total: u64,
    pub current_id: Option<String>,
}
```

约束：

- `completed <= total`。
- terminal 时 `current_id = None`。
- payload 不含 prompt、环境变量、stderr、token 或绝对用户路径。
- Tauri emit 使用完整快照；前端同时保留轮询兜底。
- 事件丢失不得导致 UI 永久 Running。

## 8. 取消与关闭

- `TaskContext::cancellation()` 是所有 worker 的唯一取消 token。
- 进程型任务取消必须继续下沉至 HostProcess 进程树终止。
- App close 只禁用冲突操作，不禁用导航、查看详情和无关 CRUD。
- 默认 graceful wait 上界沿用 TaskRuntime 配置；超时返回未清理 task 列表。

## 9. 去重与冲突键

| 工作流 | dedup key | conflict key |
|---|---|---|
| 全量 source scan | `scan:{tenant}:all:{kind}` | `catalog-write:{tenant}` |
| 单 source scan | `scan:{tenant}:{source}` | `catalog-write:{tenant}` |
| group mount | `mount-group:{tenant}:{profile}:{group}:{enabled}` | `mount-profile:{tenant}:{profile}` |
| exclusive mount | `mount-exclusive:{tenant}:{profile}:{hash(groups)}` | `mount-profile:{tenant}:{profile}` |
| conversation sync | 保持现有 scope key | 对应 record scope |
| extension lifecycle | kernel `LifecycleRequestKey` | package/resource key |

哈希输入必须排序并稳定序列化。

## 10. 测试要求

新增测试：

1. `projection_getter_returns_not_found_after_runtime_deletion`
2. `all_projection_lists_drop_orphan_running_entries`
3. `runtime_cancellation_cannot_be_overwritten_by_domain_finish`
4. `source_scan_command_returns_before_worker_completion`
5. `source_scan_deduplicates_same_scope`
6. `source_scan_progress_is_monotonic`
7. `batch_mount_loads_shared_catalog_once`
8. `batch_mount_refreshes_catalog_once_after_batch`
9. `batch_mount_cancel_stops_before_next_physical_change`
10. `batch_mount_partial_failure_has_terminal_succeeded_lifecycle`
11. `unrelated_read_command_remains_available_while_batch_runs`
12. `app_close_reports_unfinished_scan_and_mount_tasks`

## 11. 验收标准

- 生产代码存在 `TaskKind::Scan` 和 `TaskKind::BatchMount` 注册路径。
- Tauri scan/group mount 不再同步等待完整工作流。
- Engine 与 Tauri 调用同一个 workflow 实现。
- Registry 中不存在独立 lifecycle state machine。
- TaskRuntime 删除、取消、retention 后所有 UI getter 与 list 一致。
- 前端能在离开页面后通过全局任务区继续观察进度。
