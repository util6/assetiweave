# Runtime Extension Refactor 复审意见（Draft v2）

- 复审日期：2026-08-18
- 复审范围：`specs/feature-plans/runtime-extension-refactor/`
- 对照基线：`main@190bb0e` 与当前实际代码
- 结论：上一轮 15+1 项基本闭环；当前仍有 8 个实施阻塞项和 5 个次要问题，建议继续保持 **Draft v2**。

## 一、实施阻塞项

### 1. [P1] Outbox 与分批事务之间仍存在丢事件窗口

位置：`04-domain-events-outbox.md` §4 写入规则。

当前会话导入在 `src-tauri/src/backend/store/conversation_repo.rs` 中按 `chunks` 多次提交并分别推进 revision，最后还有独立的 missing/session-run 事务。若只在最终“同步提交处”写一个事件，进程可能在前面批次提交后、事件写入前退出，产生“业务数据已提交，但 outbox 不存在”的永久缺口。

建议明确选择以下一种事务模型：

1. 每个导入批次以及最终事务分别写入事件；或
2. 将整个导入改为单事务，并评估长事务对 SQLite 写锁和大批量同步的影响。

同时应让 revision bump 返回具体 revision，并由实际持有事务的 store 路径写入 outbox。`sync_conversations_with_progress` 可以构造事件语义，但不能在 store 已经提交后补写事件。

### 2. [P1] 进程内 notify 唤不醒其他进程的 dispatcher

位置：`04-domain-events-outbox.md` §5 Dispatcher。

桌面已运行时，OneShot CLI 提交的 outbox 行只会触发 CLI 进程内的 notify，ResidentHost 收不到。当前 dispatcher 除启动 catch-up 和本地 notify 外没有周期轮询，因此事件可能一直积压到桌面重启。

建议：

- notify 继续作为低延迟快速路径；
- dispatcher 同时增加低频数据库轮询和空闲退避；
- 增加集成测试：ResidentHost 持续运行，OneShot 追加 outbox 且不发送 ResidentHost 本地 notify，事件仍能在时间上界内被消费。

### 3. [P1] async Tauri command 直接调用 AppRuntime::block_on 会嵌套 Tokio Runtime

位置：`01-app-runtime.md` §6 async 姿态。

现状并非全部是同步命令。`adapters/tauri/commands.rs` 有数十个 `async fn`，其中很多特意通过 `tauri::async_runtime::spawn_blocking` 执行同步 AppService。若直接在 Tauri executor 中调用 `service.runtime.block_on`，会产生嵌套 Runtime panic，并阻塞 executor。

建议改写规则：

- 保留现有 async command 和 `spawn_blocking` 边界；
- 将 `Arc<AppRuntime>` 克隆进 blocking 闭包，再在闭包内构造 `AppService`；
- 只有同步 adapter 入口使用 `AppRuntime::block_on`；
- 增加一个 async Tauri command 的回归测试或最小运行时测试，证明不会嵌套 Runtime。

### 4. [P1] Adapter seed 顺序与 SPEC-02 相互矛盾

位置：`01-app-runtime.md` §4 第 4、5 步；`02-boundary-repairs.md` §3。

当前 `seed_tenant_defaults_sqlx` 内部直接调用 adapter seed，而 SPEC-02 又规定 `materialize_and_seed_builtin_adapters` 负责“先物化/校验、再交给 store 落库”。SPEC-01 当前顺序先执行通用 seed，再执行物化，会导致旧调用残留、缺少已物化入参或重复 seed。

建议明确拆分为：

1. 普通 tenant defaults seed，不包含 adapter；
2. 物化官方 adapter 文件；
3. 校验 adapter；
4. 调用只接收已准备数据的 store adapter seed。

还需同步修改新建 tenant、system reset 等所有 `seed_tenant_defaults_sqlx` 调用点，而不只是 bootstrap。

### 5. [P1] VersionProbe 定义的是探测结果，不是探测契约

位置：`05-extension-kernel.md` §2。

当前 `VersionProbe` 只有 `program/available/version` 等结果字段，不能描述如何执行探测。Agent 现有定义还需要 probe command override、args、env、timeout、输出上限，以及 availability/model-discovery 两类探测，因此当前类型不足以承载共享 launcher。

建议拆分：

```rust
struct ProbeSpec {
    program: Option<String>,
    args: Vec<String>,
    env: Vec<EnvEntry>,
    timeout: Duration,
    output_limit: usize,
    kind: ProbeKind,
}

struct ProbeResult {
    program: String,
    available: bool,
    version: Option<String>,
    required_version: Option<String>,
    error: Option<String>,
    hint: Option<String>,
}
```

`ProcessInvocation` 也应补齐 Agent 执行所需的 env 和 working-directory 语义。

### 6. [P1] TargetProfile 不是按列存储，当前迁移描述不会修复旧数据

位置：`06-capability-seams.md` D.2 步骤 3。

当前 `profiles` 表只有 `tenant_id/id/payload`，`TargetProfile` 整体序列化进 JSON。因此“新增 `target_provider_id` 必填列并回填 `app_kind`”与实际存储结构不符。直接给 Rust 类型增加必填字段还会使历史 payload 反序列化失败。

建议：

- 明确执行 profile JSON payload 迁移，可使用 SQLite JSON1 或 Rust 启动迁移；
- 过渡期为 `target_provider_id` 提供 serde default/兼容推导；
- 加载旧 profile 后根据 `app_kind` 推导 provider id，并重新持久化；
- `Source.origin_provider_id` 仍按普通数据库列迁移；
- 增加旧 profile payload 的升级与回滚测试。

该项也应与 `AGENTS.md:16,85` 的 SQLite 事实源及“代码/spec 不一致时以实际代码为准”保持一致。

### 7. [P1] Memory staleness 表缺少 tenant 与明确的 evidence 主键

位置：`07-event-consumers.md` §3。

`memory_id` 没有对应到现有 Memory 证据持久化模型。证据实际由 `memory_evidence_snapshots(tenant_id, id)` 标识，并可同时关联 item、dream note、run。当前旁表还缺少 `tenant_id` 和 `record_kind`，会破坏租户隔离，也无法准确表达哪条证据 stale。

建议以如下键模型为基础：

```text
memory_evidence_staleness(
  tenant_id,
  evidence_id,
  record_kind,
  source_id,
  session_id,
  stale_since_revision,
  marked_at
)
```

主键至少应覆盖 `(tenant_id, evidence_id, stale_since_revision)`，并为 evidence 设置外键及删除策略。

### 8. [P1] 现有 conversation_sync_deltas 无法按 revision 区间回查

位置：`07-event-consumers.md` §3 消费逻辑。

`conversation_sync_deltas` 只有 `tenant_id/sync_run_id/record_kind/session_id` 等字段，没有 revision，因此“按 revision 区间查询 sync deltas”没有现成实现基础。

建议：

- 使用事件已有的 `sync_run_id` 回查 deltas；
- 查询条件包含 tenant 和 record kind；
- 当前 `mark_missing_conversation_sessions_sqlx_tx` 不写 delta，应为 missing/restored 会话补写 delta；
- `changed_session_ids` 的直接载荷也必须包含 missing/restored 会话；
- 增加超过 256 条且包含 missing/restored 会话的 fallback 测试。

## 二、次要但应在批准前修正

### 9. [P2] Shutdown 的内部任务准入没有对应 API

位置：`03-task-runtime.md` §4。

第一阶段关闭统一 `spawn` 准入，但第二阶段又允许 dispatcher/consumer 提交内部任务；当前 TaskRuntime 接口只有一个 `spawn`，无法区分二者。

建议增加 `SpawnOrigin::{External, Internal}`、内部 admission token 或独立的 `spawn_internal`。测试应证明 shutdown 期间外部请求被拒绝，而受控内部任务仍能完成。若 v1 消费者不会提交任务，则删除该承诺，避免留下未实现语义。

### 10. [P2] spawn_blocking 的 JoinError 不能通过双问号自动转换

位置：`02-boundary-repairs.md` §3 示例代码。

`spawn_blocking(...).await??` 的第一个 `?` 需要从 `tokio::task::JoinError` 转换。无论此阶段 `AppResult` 仍是 String，还是局部使用 AppError，当前设计都没有相应转换。

建议显式映射，并区分取消和 panic：

```rust
let adapters = tokio::task::spawn_blocking(...)
    .await
    .map_err(map_join_error)??;
```

### 11. [P2] bootstrap 验收计数与目标实现不一致

位置：`01-app-runtime.md` §9。

目标 bootstrap 明确直接复用 `open_migrated_pool`，并要求生产路径不再经过 `Database::open_initialized`，因此生产调用次数应为 0，而不是 1。

建议分别探测并断言：

- `AppRuntime::bootstrap`：1 次；
- pool 创建：1 次；
- migration/seed/recovery：各 1 次；
- 生产 `Database::open_initialized`：0 次。

### 12. [P2] Lifecycle dedup key 会跨领域或跨操作碰撞

位置：`05-extension-kernel.md` §2。

仅使用 `package_id@version` 没有包含 `PackageKind` 和 `LifecycleOp`。同名 Agent/Conversation 包会碰撞；若各操作共用同一 TaskKind，Remove/Disable 还可能错误返回正在执行的 Install 快照。

建议：

- 资源键使用完整 `PackageIdentity`；
- 请求键包含 LifecycleOp；
- 完全相同的活动操作才 dedup；
- 不同但冲突的操作按明确冲突矩阵返回 `AppError::Conflict`。

### 13. [P2] 新消费者从 0 开始不能恢复已经清理的历史

位置：`04-domain-events-outbox.md` §3 保留策略。

系统运行超过保留期后再发布新消费者时，历史 outbox 可能已被旧消费者水位清理。此时把新消费者的 `last_seq` 初始化为 0，也无法重新取得已删除事件，因此“新注册消费者不丢事件”的验收不能只靠 offset 初始化实现。

建议要求新增消费者提供：

1. 从领域源表生成 snapshot/backfill；
2. 记录 backfill 对应的 cutoff seq；
3. 在同一迁移/注册流程中把 offset 初始化为 cutoff seq；
4. 从 cutoff 后继续正常消费 outbox。

只有首次随 outbox 一起注册、且历史尚未清理的消费者适合从 0 开始。

## 三、复审结论

上一轮重点问题中，以下部分已经完成有效修订：

- ResidentHost/OneShot 进程角色；
- CLI 长操作保持同步；
- shutdown 阶段顺序；
- consumer 完成后再推进 offset；
- TrustGate 保留领域信任态；
- Memory 旧键扇出；
- TargetProvider 对 AppKind 的解耦方向；
- canonical_method 与 SurfaceMapping 的事实修正。

建议处理顺序：

1. 先处理 Outbox 分批事务与跨进程唤醒；
2. 再修正 AppRuntime 的 async 边界和 bootstrap seed 顺序；
3. 完成 Extension Kernel 探测契约；
4. 修正 TargetProfile 与 Memory 的实际存储模型；
5. 最后清理 P2 验收和接口细节。

以上 P1 项关闭后，可进入 Approved 候选复审。
