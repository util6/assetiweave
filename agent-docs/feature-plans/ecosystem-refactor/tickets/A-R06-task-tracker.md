# A-R06：TaskTracker 替换手工活动计数与 Condvar

**Depends:** A-R04
**Contracts:** C-BASE、C-TASK、C-ERROR
**Outcome:** 唯一 TaskRuntime 继续拥有任务语义，tokio-util 只接管任务寿命追踪与关闭等待。

## 执行规则

状态：`PLANNED`。先读总入口、本卡 Contract IDs、`../02-dependencies.md`、`../05-playbook.md`。一轮只做本卡。原有正确行为先 characterization green；随后新增 adoption/deletion guard 得到 red，再迁移。筛选测试先 `-- --list`，零测试不算 green。只用临时目录/内存库/loopback fixture；本卡不授权插件架构或真实用户数据操作。

## 文件

- Modify：`src-tauri/src/backend/runtime/tasks.rs`、`runtime/app_runtime.rs`、`runtime/tests.rs`；`src-tauri/src/backend/application/memory_recall_workflow.rs` 中直接调用 TaskRuntime shutdown的测试；`src-tauri/Cargo.toml`、`Cargo.lock`。
- Create：无。Test：`runtime/tests.rs`；已有 AppRuntime shutdown消费者保持签名。

## 接口

Consumes：既有 `TaskSpec`、`TaskSnapshot`、`CancellationToken`、`TaskRuntime::spawn/register_external/finish_external` 和 AppRuntime 持有的 Tokio runtime。

Produces（修改内部接口）：

```rust
// 从同步方法改为async；外部Wire DTO不变。
pub(crate) async fn shutdown_with_grace(&self, grace: Duration) -> ShutdownReport;
// TaskRuntime增加 tracker: tokio_util::task::TaskTracker
// TaskEntry增加 tracking: Option<tokio_util::task::TaskTrackerToken>
```

`AppRuntime::shutdown_with_grace(&self, Duration) -> ShutdownReport` 保持同步，以其**已有** `self.block_on(...)` 驱动 TaskRuntime shutdown。`stop_session_memory_coordinator` 中另一处任务等待同样接已有runtime。测试对直接TaskRuntime调用用测试runtime `.block_on`；生产不新增runtime。

## 步骤

- [ ] 跑 `runtime::tests`，保存去重、external task终态、取消、退出宽限期的green。
- [ ] 加source guard得到red；这是替换机制的强制证据：

```rust
#[test]
fn task_runtime_uses_tracker_instead_of_condvar_accounting() {
    let source = include_str!("tasks.rs");
    assert!(source.contains("TaskTracker"));
    assert!(!source.contains(concat!("Cond", "var")));
    assert!(!source.contains(concat!("fn release_active_", "slot(")));
}
```

- [ ] 在成功注册任务且持有tasks锁时创建 `tracker.token()`；同一锁内检查accepting，避免stop/新注册竞态。Pending任务token存entry；启动普通任务时token取出移到worker，worker真正返回时Drop；external task token由finish_external唯一消费。
- [ ] 普通worker继续catch_unwind并发布终态；token用RAII无论成功、取消、panic都释放。不要在“UI已终态”但worker还执行清理时提前drop。取消请求不释放token，终止真正完成才释放。
- [ ] `stop_accepting` 在同一注册锁内设置闸门并close；tracker.close本身不禁止spawn，注册函数的闸门必须保留。关闭等待核心：

```rust
self.stop_accepting();
{
    let tasks = self.tasks.lock().unwrap_or_else(|poison| poison.into_inner());
    for entry in tasks.values().filter(|entry| entry.snapshot.state.is_active()) {
        entry.cancellation.cancel();
    }
}
self.tracker.close();
let _ = tokio::time::timeout(grace, self.tracker.wait()).await;
// 然后由既有snapshot枚举unfinished_task_ids；超时不捏造任务已结束。
```

- [ ] 删除 `active: Arc<(Mutex<usize>, Condvar)>`、注册加计数、release_active_slot及所有notify/wait_timeout分支；保留tasks map、conflict_keys、进度、租户、sequence、terminal retention。
- [ ] 用确定的channel/barrier测试注册与shutdown竞态、external task未finish的超时报告、panic释放token、取消后清理完成再退出。额外测试 `tracker.close()` 后原注册入口拒绝任务，避免误依赖库close行为。

## 具体行为测试骨架

```rust
#[test]
fn shutdown_waits_for_external_task_completion() {
    let tasks = tasks::TaskRuntime::new();
    let spec = tasks::TaskSpec::new(tasks::TaskKind::Other, None)
        .with_task_id("external-still-running");
    tasks.register_external(spec).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
    let report = rt.block_on(tasks.shutdown_with_grace(Duration::from_millis(5)));
    assert_eq!(report.unfinished_task_ids, vec!["external-still-running"]);
    assert!(tasks.spawn(
        tasks::TaskSpec::new(tasks::TaskKind::Other, None),
        Box::new(|_| Ok(serde_json::Value::Null)),
    ).is_err());
}
```

## 验证与停止

```bash
cargo test -p assetiweave --lib backend::runtime::tests -- --nocapture
cargo test -p assetiweave --lib memory_recall_workflow
cargo test -p assetiweave --lib background_tasks -- --test-threads=1
cargo fmt --all -- --check
```

成功：guard绿、手工计数删净、退出宽限/未完成ID/任务去重保真；TaskTracker不成为第二任务权威。停止：拟在TaskRuntime里新建运行时、把Drop当取消、直接abort阻塞任务、清空任务map来伪造关闭完成。

[官方 API：TaskTracker](https://docs.rs/tokio-util/latest/tokio_util/task/struct.TaskTracker.html)
