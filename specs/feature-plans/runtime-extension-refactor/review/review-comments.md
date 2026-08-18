# Runtime & Extension Refactor SPEC 审计评论

- 审计日期：2026-08-18
- 审计基线：`main@190bb0e`
- 审计范围：`specs/feature-plans/runtime-extension-refactor/`
- 结论：发现 15 个执行前应修订的问题，其中 9 个会直接导致方案不可实现或破坏现有行为。建议修复 P1 后再将文档状态标记为 `Approved`。

## P1：执行前必须解决

### 1. AppError 第一步无法保持编译兼容

位置：`01-app-runtime.md:142-145`

仅增加 `From<String>` 并修改 `AppResult` 别名不会兼容现有代码：

- 大量 `return Err(String)` 不会自动转换；
- Tauri command 要求错误可序列化，而当前设计没有 wire adapter；
- Engine 的 `DispatchFailure::App` 目前接收 `String`。

建议先定义 Tauri/Engine 边界的 `WireError` 映射，再按模块迁移内部返回类型，不能把本步骤描述成可独立全仓编译通过。错误契约变更还需遵守 `AGENTS.md:17`。

### 2. 进程内 TaskRuntime 无法被 CLI 跨调用查询

位置：`03-task-runtime.md:64-75`

当前每次 `EngineClient.Call` 都启动新的 `assetiweave-engine` 子进程并等待退出；Engine 也只读取一个 stdin 请求后结束。因此启动后台同步的进程退出后，下一次 `task.list`/`task.cancel` 会连接到全新的空 TaskRuntime，所列 CLI e2e 不可实现。

建议先裁决一种运行模型：

1. 持久化任务状态并引入常驻 worker；
2. 引入长生命周期 Engine/daemon；
3. 明确 CLI 继续同步执行，任务方法只服务同一常驻宿主。

相关代码：`cli/internal/client/engine.go:70`、`src-tauri/src/adapters/engine/transport.rs:87-136`；相关规则：`AGENTS.md:14,56,70`。

### 3. Shutdown 顺序会先终止 Outbox dispatcher

位置：`03-task-runtime.md:57-62`

SPEC-04 把 dispatcher 定义为 TaskRuntime 常驻任务，但这里先执行 `TaskRuntime.shutdown`，再执行 dispatcher drain。前一步会停止或取消 dispatcher，后一步已经没有执行者；消费者也可能无法再提交内部索引任务。

建议改成分阶段关闭：

```text
停止外部任务准入
  → dispatcher catch-up/drain（仍允许内部任务）
  → 停止消费者
  → shutdown TaskRuntime
  → 关闭数据库连接池
```

该顺序还应覆盖 `AGENTS.md:69` 的退出保护要求。

### 4. Outbox 缺少跨进程 dispatcher 所有权

位置：`04-domain-events-outbox.md:88-96`

SPEC-01 要求 Tauri 和每个 Engine 进程都 bootstrap `AppRuntime`，而 SPEC-04 默认每个 `AppRuntime` 都启动 dispatcher。桌面与一次性 Engine 同时访问同一 SQLite 时会并发领取同一批事件；TaskRuntime 去重又只在进程内，可能并发重建索引或重复执行昂贵消费者。

必须明确以下两种方案之一：

- 只有指定 host 启动 dispatcher；
- 为每个 `consumer × tenant` 增加数据库 lease/claim。

一次性 Engine 还需明确是否同步 catch-up，以及何时允许进程退出。

### 5. RuntimeLauncher 无法表达现有 Adapter 运行时

位置：`05-extension-kernel.md:63-68`

当前 `ConversationAdapterRuntimeKind` 支持 `Node`、`Python`、`Bash`、`Executable`，并携带 args、版本探测和平台相关程序解析。新设计的 `Node/HostProcess + entry` 没有 args，也没有 Python/Bash/Executable 的解释器和 probe 语义。

按后续迁移步骤接入会丢失合法 manifest 行为。应补齐通用 `ProcessInvocation`/`VersionProbe` 描述，或明确 launcher 保持领域专属，不在 kernel 统一。

### 6. Legacy memory assignment 迁移会断开 Dream

位置：`06-capability-seams.md:48-52`

当前同一个 `agentCapabilityAssignments.memory` 同时被 `memory_extraction.rs` 和 `memory_dream.rs` 使用。若只将旧键映射成 `memory.extraction`，随后 Dream 查询 `memory.dream` 时将没有 agent 配置，现有用户的 Auto-Dream 会变为不可用。

迁移必须把旧值扇出到至少：

- `memory.extraction`
- `memory.dream`

同时同步前端 settings schema；若用户以后分别配置，再由新键覆盖迁移值。

### 7. 新 TargetProvider 无法构造 TargetProfile

位置：`06-capability-seams.md:130-134`

当前 `TargetProfile.app_kind` 是必填 `AppKind`。本 SPEC 又规定：

- 新 provider 的 `app_kind_compat=None`；
- 不得新增 `AppKind` 变体；
- 不得使用 `AppKind::Custom`。

仅增加 `target_provider_id` 后，虚构 App 仍无法构造或反序列化 `TargetProfile`，因此第 4 步 e2e 在类型层面不可实现。

需要把 `app_kind` 改成兼容性的 `Option<AppKind>`/`AppKindCompat` 字段，并迁移数据库与 DTO；同时检查 `Source.origin_app_kind` 是否需要对应的 provider id。

### 8. 异步索引重建与消费位点提交语义不完整

位置：`07-event-consumers.md:44-48`

v1 允许消费者只触发 TaskRuntime 重建任务，但下一条又要求 `indexed_revision` 与 consumer offset 同事务推进：

- 如果 `handle` 在任务入队后返回，进程可能在重建完成前推进 offset，导致永久漏更新；
- 如果同步等待，则需明确 completion handle、取消和 shutdown 行为。

建议规定 offset 只能由成功完成重建的任务提交，或者由消费者同步执行增量，并在同一事务更新两类游标。

### 9. canonical_method 的基线判断与代码相反

位置：`08-interface-coverage.md:9-16,42-47`

当前 249 个契约条目的 `canonical_method` 全部非空：

- 147 个与 `method` 相同；
- 102 个 alias 指向 canonical；
- 共 169 个唯一 canonical。

Go protocol 会读取该字段，CLI e2e 也断言 `source.remove` 的 canonical 值。方案 A 会破坏 alias、policy 和 invocation metadata。

应删除方案 A，把 `canonical_method` 作为 capability 聚合键生成矩阵；`CommandMeta` 也应支持一个 canonical 对多个 Engine aliases。依据 `AGENTS.md:83-85`，此处应以当前代码和生成契约为准。

## P2：应在对应阶段实施前修订

### 10. Bootstrap API 仍依赖将被测试化的 Database

位置：`02-boundary-repairs.md:42-54`

这里要求 `materialize_and_seed_builtin_adapters` 接收 `&Database` 并调用 `db.block_on`，但 SPEC-01 明确要求生产 `AppRuntime` 只持有 `SqlitePool`/tokio runtime，并把 `Database` 降为测试与迁移工具。两个 SPEC 合流后，`AppRuntime` 没有可传入的 `Database`。

建议把函数设计为 async，只接收 `&SqlitePool` 和已物化数据，由 bootstrap 在外层 await。

### 11. Projection 守卫没有执行声明的允许列表

位置：`02-boundary-repairs.md:88-89`

第 35 行规定 projection 只能依赖 `models`/`dto`/标准库/serde，但 R4 只禁止 `store`、`application`、`capabilities`。导入 `conversations`、`scanner`、`planner`、`executor` 或直接使用 `std::fs` 都能通过 CI。

应改成完整禁止列表，或解析 Rust `use` 路径实施允许列表，否则边界可以立即回潮。

### 12. Retention 会忽略缺失的消费者位点

位置：`04-domain-events-outbox.md:48-57`

仅对已有 offset 行取 `min(last_seq)` 时，新注册消费者或新 tenant 尚未创建 offset 的情况不会参与计算，可能删除尚未消费的事件；当前文本也没有说明按 tenant 分组。

建议：

1. 注册 consumer 或创建 tenant 时，为每个 `registered consumer × tenant` 写入初始 offset；
2. 按 tenant 计算安全删除水位；
3. 增加缺失 offset、新增 consumer、新增 tenant 的 retention 测试。

### 13. TrustState 不能保序映射现有 Changed 状态

位置：`05-extension-kernel.md:58-61`

现有 `ConversationAdapterTrustState` 是：

- `BuiltIn`
- `Trusted`
- `Changed`
- `Untrusted`

新枚举则是 `BuiltIn`、`Verified`、`Imported`、`Untrusted`。`Changed` 表示已信任内容发生哈希变化，不能无损映射成 `Imported` 或 `Untrusted`，否则 UI 与启用门禁会丢失重要安全状态。

应保留 `Changed`/`Trusted` 语义，或让领域 trust state 保持专属，kernel 只抽取通用判定接口。

### 14. Memory stale 标记粒度会误伤同源未变化证据

位置：`07-event-consumers.md:59-64`

只按 `source_id + revision` 查 Memory 记录，会把同一 source 下未变化 session 的证据一并标记 stale。事件已经提供 `changed_session_ids`；超限时也能按 revision 查询 `conversation_sync_deltas`。

消费者至少应按 `record_kind + session_id` 关联 evidence，必要时进一步比较 `question_id`、`block_id` 或 `content_hash`。

### 15. 内存时间戳与持久化 offset 不一致

位置：`07-event-consumers.md:72-74`

该消费者处理成功后会推进持久化 offset，但唯一效果只存在内存。进程重启后时间戳丢失，事件又因 offset 已推进而不会重放，违反 at-least-once/可恢复语义。

应将 wake timestamp 持久化并幂等更新，或者删除这个占位消费者，使用测试假消费者验证多游标隔离。

## 补充设计问题

### CommandMeta 目前会成为第三份风险定义

位置：`08-interface-coverage.md:18-40`

`CommandMeta` 重新声明 `risk`/`confirmation_required`，但实施步骤只要求 Tauri 逐步读取它，没有规定 Engine registry 的现有 `CommandRisk`/confirmation 如何由它生成或引用。因此所谓单一元数据源会实际变成长期重复定义。

应选择一种明确结构：

1. Engine `CommandSpec` 从 `CommandMeta` 组合生成；或
2. 矩阵直接消费现有 Engine registry 元数据，只为 Tauri 增加 canonical capability 映射。

## 建议处理顺序

1. 先修正 CLI/Engine 生命周期与 TaskRuntime 模型。
2. 裁决 dispatcher host、跨进程 lease 和 shutdown 顺序。
3. 修正 AppError wire contract 与可编译迁移步骤。
4. 补齐 Extension Kernel 对现有 runtime/trust 语义的无损表达。
5. 修正 Action settings 与 TargetProvider 的数据迁移模型。
6. 修正事件消费者的 offset、幂等和持久化语义。
7. 以现有 `canonical_method` 为基础重写接口覆盖章节。
8. 完成上述 P1 后，再将各 SPEC 状态标记为 `Approved`。
