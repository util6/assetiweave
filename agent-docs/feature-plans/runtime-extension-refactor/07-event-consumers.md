# SPEC-07:首批领域事件消费者(P2)

- 状态:Draft v3(v1 审计 #8/#14/#15;v2 复审 #7/#8 修订)
- 前置:SPEC-04(outbox 与 dispatcher 已落地)
- 进程模型假设(SPEC-00 §3a):消费者只在 ResidentHost 的 dispatcher 内运行;OneShot 进程与 CLI-only 场景由本篇 §5 的**常设 pull 兜底**保证读新鲜度。
- 交付物:消费者契约、消费者一(搜索索引推进)、消费者二(Memory evidence stale 标记)
- 边界:本篇 MUST NOT 涉足 Memory 功能重做;stale 标记是"为未来重做铺轨",只写标记不改任何 Memory 行为。

---

## 1. 消费者契约

`backend/events/consumer.rs`:

```rust
pub(crate) trait DomainEventConsumer: Send + Sync {
    /// 全局唯一、稳定;写入 domain_event_consumer_offsets.consumer_id,MUST NOT 改名。
    fn id(&self) -> &'static str;
    fn interested(&self, event: &DomainEvent) -> bool;
    /// 幂等(MUST):同一批次重复投递结果一致。批内按 seq 升序。
    /// 语义(修订,审计 #8):`handle` 返回 Ok 即表示该批的业务效果**已完成并已提交**;
    /// dispatcher 在其返回后才推进 offset。MUST NOT "把工作丢进队列即返回 Ok"。
    /// 返回 Err → dispatcher 按 SPEC-04 §5 重试;MUST NOT 部分提交后报错——
    /// 要么整批业务效果幂等可重放,要么内部自管子游标。
    fn handle(&self, batch: &[SequencedEvent], cx: &ConsumerCx) -> Result<(), AppError>;
}
pub(crate) struct SequencedEvent { pub seq: i64, pub event: DomainEvent }
pub(crate) struct ConsumerCx { /* pool 访问、tenant、cancellation token(挂 TaskRuntime) */ }
```

注册:`AppRuntime::bootstrap(ResidentHost)` 时静态注册(编译期清单);注册即触发 SPEC-04 §3 的 offset 行初始化,并参与保留水位计算。注册时 MUST 声明 `InitialPosition`(SPEC-04 §3.5):随 outbox 首发合入的消费者用 `GenesisZero`,后发消费者用 `BackfillThenCutoff`。注销一个消费者 = 代码删除 + 迁移清理其 offsets 行(写入迁移文件)。

消费者矩阵(本文档维护,新增消费者 MUST 更新;审计 #15:原第三个占位消费者已删除,见 §4):

| consumer_id | 订阅 | 效果 | 幂等键 |
|---|---|---|---|
| `search.index_advance` | ConversationSourceCommitted | 推进全文索引 | revision 区间 |
| `memory.evidence_stale` | ConversationSourceCommitted | 按会话粒度标记受影响 Memory 证据 stale | (tenant_id, evidence_id, stale_since_revision) |

## 2. 消费者一:搜索索引推进 `search.index_advance`

### 现状

`search/conversation/lifecycle.rs`:索引健康检查以 `state.health == "ready" && state.indexed_revision == Some(state.source_revision)` 判断是否可用;推进依赖显式重建任务/使用时检查,同步提交后无主动推进。

### 规范

1. **同步完成语义(修订,审计 #8)**:`handle` 在消费者调用栈内**同步完成**推进后才返回——dispatcher 本身已是后台任务,不阻塞 UI。MUST NOT "入队重建任务后立即返回 Ok":那会让 offset 先于重建完成被推进,崩溃窗口内产生永久漏更新。若未来确需委托 TaskRuntime 任务执行,offset 的提交 MUST 移入该任务的成功完成路径(与 `indexed_revision` 同事务),`handle` 对该批挂起——v1 不实现挂起协议,故 v1 一律同步执行。
2. **游标合一(单脊柱落地)**:推进成功的同一事务内更新 `indexed_revision` 与 `domain_event_consumer_offsets('search.index_advance').last_seq`;`indexed_revision` 保留为"索引覆盖到哪"的语义游标,其推进 MUST 只发生在本消费者路径与显式重建路径两处,禁止第三处写入。
3. 失败:索引不可用/不兼容时走现有 `mark_index_unusable` 路径,消费返回 Ok 并允许 offset 推进——此时增量事件对不可用索引无意义,后续显式重建会把 `indexed_revision` 直接对齐 `source_revision`,不依赖事件重放;MUST NOT 无限重试。
4. 验收观测:复用 `ConversationSearchIndexRebuildReport`。

### 测试

- `consumers::search::tests::commit_event_advances_index`(同步一批→事件→索引 revision 追平);
- `consumers::search::tests::replay_is_idempotent`(同批投递两次,文档数与 revision 不变);
- `consumers::search::tests::offset_not_advanced_before_index_commit`(注入推进失败,断言 offset 未动);
- `consumers::search::tests::unusable_index_does_not_retry_forever`。

## 3. 消费者二:Memory evidence stale 标记 `memory.evidence_stale`

### 规范

1. 迁移文件(修订,v2 复审 #7,对齐真实证据模型——证据由 `memory_evidence_snapshots(tenant_id, id)` 标识,自带 record_kind/session_id/question_id/block_id/content_hash,见 `migrations/202607230001`):

   ```sql
   CREATE TABLE memory_evidence_staleness (
       tenant_id            TEXT NOT NULL,
       evidence_id          TEXT NOT NULL,   -- 引用 memory_evidence_snapshots.id
       record_kind          TEXT NOT NULL,
       source_id            TEXT,
       session_id           TEXT NOT NULL,
       stale_since_revision INTEGER NOT NULL,
       marked_at            TEXT NOT NULL,
       PRIMARY KEY (tenant_id, evidence_id, stale_since_revision),
       FOREIGN KEY (tenant_id, evidence_id)
           REFERENCES memory_evidence_snapshots(tenant_id, id) ON DELETE CASCADE
   );
   ```

   旁表方案保持——不动 Memory 现有 schema,与"Memory 将重做"兼容;`tenant_id` 为主键首列(缺失会破坏租户隔离,v2 复审 #7)。
2. **消费逻辑(修订,v1 #14 会话粒度 + v2 #8 回查通道)**:先定位变更**会话集合**——优先取事件的 `changed_session_ids`;为 `None`(超限)时按事件携带的 `sync_run_id` 回查 `conversation_sync_deltas`(条件含 `tenant_id` 与 `record_kind`;该表**无 revision 列**,MUST NOT 按 revision 区间查询)。随后按 `(tenant_id, record_kind, session_id)` 关联 `memory_evidence_snapshots`,对命中证据写 stale 行;MAY 进一步用 `question_id`/`content_hash` 收窄。MUST NOT 按 `source_id` 一刀切。**只写标记**:不触发提取、不改 recall 行为、不发通知。
   **前置依赖(v2 复审 #8)**:`mark_missing_conversation_sessions_sqlx_tx` 现不写 delta,且 deltas 的 `change_kind` 有 `CHECK (IN ('new','updated'))` 约束——MUST 先落一条迁移放宽 CHECK(增 `missing`/`restored`),并让 missing/restored 路径写 delta;事件的 `changed_session_ids` 同样 MUST 含 missing/restored 会话(SPEC-04 §4.3)。
3. 读侧(可选,MAY):Memory 卡片 DTO 增加只读 `stale: bool` 字段供 UI 显示;不改任何交互。
4. Memory 重做时,该旁表与消费者是现成输入;重做方案自行决定去留。

### 测试

- `consumers::memory::tests::commit_marks_changed_sessions_evidence_stale`;
- `consumers::memory::tests::unchanged_sessions_in_same_source_untouched`(v1 #14 的误伤回归);
- `consumers::memory::tests::capped_event_falls_back_to_deltas_by_sync_run_id`(v2 #8:超 256 条且含 missing/restored 会话的 fallback);
- `consumers::memory::tests::tenant_isolation_of_staleness_rows`(v2 #7);
- `consumers::memory::tests::replay_is_idempotent`。

## 4. 已删除:Auto-Dream eligibility 占位消费者(修订,审计 #15)

原设计("内存态记录唤醒时间 + 持久化 offset")违反 at-least-once 可恢复语义:重启后效果丢失且事件因 offset 已推进不再重放。且多消费者游标隔离已由 SPEC-04 §6 的**测试假消费者**覆盖,无需生产占位。`memory.dream_eligibility` 延后至 Memory 重做时按真实需求设计(彼时其效果 MUST 持久化)。

## 5. 常设 pull 兜底(修订,配合审计 #4 的指定宿主方案)

现有"打开搜索页触发检查/重建"的 pull 路径 MUST **常设保留**,不设移除时间表:OneShot 进程与 CLI-only 场景没有 dispatcher,读新鲜度由该路径兜底;事件机制是常驻宿主的加速器,不是唯一路径。pull 路径与消费者一共享同一推进实现,防止两处算法漂移。

## 6. 实施顺序与验收

1. 契约 + 注册机制,用 SPEC-04 §6 的测试假消费者验证 dispatcher 全链路(多游标隔离、重试互不阻塞)。
2. 消费者一(含游标合一事务),端到端:`同步 → outbox → dispatcher → 索引推进 → 搜索命中新会话`(集成测试,临时 DB + 样本 adapter 数据,复用 `conversations/tests.rs` 的样本构造)。
3. 消费者二。
4. 更新本文件 §1 消费者矩阵(SPEC-04 §7 要求)。

全局验收:
- 两个消费者 offsets 独立推进;kill 掉 dispatcher 再启动,catch-up 后追平(集成测试)。
- pull 兜底回归:禁用 dispatcher(测试构造 OneShot 角色)时,打开搜索路径仍能推进索引。
- `cargo test --workspace` 全绿;冒烟:同步一个真实 Codex 源后,不做任何手动操作,搜索能命中新会话。
