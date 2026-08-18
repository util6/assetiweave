# SPEC-04:领域事件与 Outbox 单脊柱(P1 末)

- 状态:Draft v3(v1 审计 #3/#4/#12;v2 复审 #1/#2/#13 修订)
- 进程模型假设(SPEC-00 §3a):任何进程可在事务内**追加** outbox 行;**只有 ResidentHost 派发**(§5)。
- 前置:SPEC-01(dispatcher 宿主)、SPEC-03(dispatcher 以任务形态运行、关闭 drain)
- 交付物:`domain_event_outbox` 迁移、`backend/events/` 模块、AppRuntime 内 dispatcher、conversation 提交点接线
- 消费者接入见 SPEC-07(本篇不实现任何消费者业务)

---

## 1. 事件三分类(全仓公约)

| 类型 | 语义 | 可靠性 | 通道 |
|---|---|---|---|
| Progress event | 任务进度提示 | 可丢失,轮询补偿 | TaskRuntime 快照 + Tauri emit(SPEC-03) |
| **Domain event** | **已提交的业务事实** | **不得静默丢失** | **同事务 outbox + dispatcher(本篇)** |
| Transport event | Tauri/Engine 传输形式 | 按传输语义 | adapter 层,MUST NOT 携带业务判定 |

MUST NOT 混用:进度不进 outbox;domain event 不靠 Tauri emit 保证送达。

## 2. 单脊柱原则(本篇核心约束)

现状:派生数据各自追赶 `source_revision` —— 搜索索引用 `conversation_search_index_state.indexed_revision`(`search/conversation/lifecycle.rs`),Memory 用 preview 的 `source_revision_start/end`(`application/memory_dream.rs`)。事实的持久性已由该游标模型保证;缺的是**统一的变更账本、统一的消费者位点、以及提交后的及时唤醒**。

因此:

1. **Outbox 是 revision 游标机制的显式化,MUST NOT 成为与它并列的第二套传播系统。** outbox 行的排序与 `source_revision` 单调对应;消费者位点统一进 `domain_event_consumer_offsets` 表;既有的 `indexed_revision` 是第一个迁移对象(SPEC-07),Memory 的 preview 游标在 Memory 重做时迁移(本轮不动)。
2. 交付语义:**at-least-once**。消费者 MUST 幂等(SPEC-07 契约)。
3. 漏唤醒不是丢事实:dispatcher 崩溃/退出后,重启时从 offsets 追赶(catch-up),MUST 有启动追赶步骤。

## 3. 数据模型

迁移文件 `src-tauri/migrations/<YYYYMMDDNNNN>_domain_event_outbox.sql`:

```sql
CREATE TABLE domain_event_outbox (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,   -- 全局单调派发序
    event_id       TEXT    NOT NULL UNIQUE,             -- "evt-" + ulid
    tenant_id      TEXT    NOT NULL,
    event_type     TEXT    NOT NULL,                    -- 枚举变体名 snake_case
    source_id      TEXT,                                -- 会话源事件必填
    revision_start INTEGER,                             -- 含
    revision_end   INTEGER,                             -- 含;单点事件 start=end
    payload        TEXT    NOT NULL,                    -- 变体字段 JSON(见 §4)
    created_at     TEXT    NOT NULL
);
CREATE INDEX idx_outbox_tenant_seq ON domain_event_outbox(tenant_id, seq);

CREATE TABLE domain_event_consumer_offsets (
    consumer_id  TEXT NOT NULL,
    tenant_id    TEXT NOT NULL,
    last_seq     INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL,
    PRIMARY KEY (consumer_id, tenant_id)
);
```

保留策略(修订,审计 #12):
1. **初始化**:ResidentHost bootstrap 时与新建 tenant 时,为每个已注册 consumer × tenant 写入 `last_seq=0` 的初始 offset 行;计算水位时**缺行一律视为 0**(安全默认,阻止删除)。
2. **水位**:按 tenant 计算 `safe_seq(tenant) = min(该 tenant 下全部已注册消费者的 last_seq;任一消费者缺行则为 0)`。
3. **清理**:dispatcher 空闲时删除 `tenant_id 匹配 且 seq < safe_seq(tenant) 且 created_at 早于 30 天` 的行。
4. 测试 MUST 覆盖三种场景均不丢未消费事件:消费者缺 offset 行、新注册消费者、新建 tenant。
5. **后发消费者注册协议(修订,v2 复审 #13)**:运行超过保留期后新增的消费者,历史 outbox 可能已被旧消费者水位清理——`last_seq=0` 起步救不回已删除事件。注册时 MUST 声明 `InitialPosition`:
   - `GenesisZero`:仅当该消费者随 outbox 机制首发合入、历史尚未清理时合法;
   - `BackfillThenCutoff`:先从领域源表全量重建效果(两个 v1 消费者的 backfill 恰好就是各自现成的 pull 全量路径——索引重建、stale 全量扫描),记录注册时的 `cutoff = max(seq)`,offset 初始化为 cutoff,自 cutoff 后进入正常消费。backfill 与 offset 初始化 MUST 在同一注册/迁移流程内完成。

## 4. 事件类型(v1 只有一个,强类型)

`backend/events/mod.rs`:

```rust
/// 领域事件。MUST 保持封闭枚举;新增变体走本 SPEC 的评审清单(§7)。
/// MUST NOT 退化为 topic + serde_json::Value。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub(crate) enum DomainEvent {
    ConversationSourceCommitted {
        event_id: String,
        tenant_id: String,
        sync_run_id: String,          // 复用 deltas 已有概念(dto/types.rs)
        source_id: String,
        revision_start: i64,          // 本次提交覆盖的 revision 闭区间
        revision_end: i64,
        /// 便利字段:变更会话 id,MUST 封顶 256 条;超限置 None,
        /// 消费者按区间查 deltas 表自取(已有查询能力)。
        changed_session_ids: Option<Vec<String>>,
    },
}
```

写入规则(MUST;修订,v2 复审 #1——会话导入是**分批多事务**:每批独立 commit 且每批 bump revision,另有收尾 missing 事务,见 `store/conversation_repo.rs` 的 `chunks(CONVERSATION_IMPORT_BATCH_SIZE)` 路径):
1. **每个提交 revision 的事务各写一行事件**:批次事务与收尾事务内、`bump_conversation_search_source_revision_sqlx_tx` 之后追加 outbox insert——"数据已提交而事件缺失"的窗口即被排除。事件粒度=事务;`revision_start/end` 取该事务 bump 前后的值(bump 函数 MUST 改为返回具体 revision)。
2. 写入职责分工:事件的**构造语义**(sync_run_id、变更集合)由应用层准备并传入;**追加动作**由持有事务的 store 路径执行(SPEC-02 事件追加边界注记)。MUST NOT 在 store 提交之后由应用层补写事件。
3. 收尾事务的事件 MUST 覆盖 missing/restored 会话(其 `changed_session_ids` 即该集合;delta 与 CHECK 约束的配套迁移见 SPEC-07 §3.2 前置依赖)。
4. MUST NOT 在事务提交前同步调用任何消费者。
5. 事件唯一来源仍是 conversation 同步链路;其他模块想发事件 = 走 §7 新增变体流程,禁止绕过。

## 5. Dispatcher

`backend/events/dispatcher.rs`,由 `AppRuntime` 持有、以 TaskRuntime 常驻任务运行:

**所有权(修订,审计 #4)**:dispatcher 仅在 `RuntimeRole::ResidentHost` 进程启动(当前即 Tauri 桌面进程)。`OneShot` 进程 MUST NOT 启动 dispatcher——它只在事务内追加 outbox 行,由下次运行的常驻宿主 catch-up 消化;CLI-only 场景的读新鲜度由 SPEC-07 的**常设 pull 兜底**保证。跨进程 lease/claim 方案(每 consumer × tenant 的 DB 租约)留待 daemon RFC,v1 MUST NOT 实现。桌面进程与 CLI 并发运行时,因只有桌面派发,不存在双 dispatcher 争抢;消费者幂等(§2)另兜住极端窗口。

1. 启动:对每个注册消费者执行 catch-up(`last_seq` → 当前 max seq,分批,批大小 100)。
2. 运行(修订,v2 复审 #2):进程内 `notify()` 是**低延迟快路径**,只能唤醒同进程的 dispatcher;OneShot CLI 写入的事件对 ResidentHost 不可见。因此 dispatcher MUST 同时维持**低频数据库轮询**:起步间隔 2–5s,空闲指数退避至 ~30s;每次轮询是一条按 seq 的索引查询,空转成本可忽略。被 notify 唤醒或轮询命中后拉取新行,按 seq 升序、逐消费者投递。
3. 投递:调用消费者 `handle(batch)`(SPEC-07 契约);成功→原子更新该消费者 `last_seq`;失败→指数退避重试(1s、5s、25s,上限 5 分钟循环),**不阻塞其他消费者**(每消费者独立游标与重试状态)。
4. 关闭:遵循 SPEC-03 §4 的分阶段顺序——dispatcher 的末次派发/drain 发生在停止外部任务准入**之后**、TaskRuntime shutdown **之前**(修订,审计 #3),报告未投递数(进 ShutdownReport)。
5. 可观测:dispatcher 状态暴露为 TaskRuntime 的常驻任务快照(投递位点、滞后量、重试中消费者)。

## 6. 实施步骤

1. 迁移 SQL + `DomainEvent` 类型 + outbox 写入函数(带事务参数),单测:同事务原子性(注入失败回滚后 outbox 无行)。
2. 提交点接线(按 §4 修订版):批次事务与收尾事务**各自**追加事件;`bump_conversation_search_source_revision_sqlx_tx` 改为返回 revision;`changed_session_ids` 封顶且含 missing/restored;此时无消费者,仅积累行——验证保留策略不清理(无注册消费者时不删行,直到 SPEC-07 注册首个)。
3. Dispatcher + 空消费者注册表 + catch-up;以假消费者(测试内)验证 §5 全部行为。
4. 与 SPEC-03 shutdown 串接。

## 7. 新增事件变体的评审清单(写进代码注释)

新变体 MUST 满足:(a) 表达**已提交**事实,有明确的事务写入点;(b) 载荷可从持久化数据重建(事件丢了能 catch-up);(c) 有至少一个已立项消费者;(d) 字段含 event_id/tenant_id 与区间或单调序;(e) 更新 SPEC-07 的消费者矩阵文档。不满足则用 progress/transport 通道或不发事件。

## 8. 验收

- 测试:
  - `events::tests::outbox_row_commits_atomically_with_business_write`;
  - `events::tests::changed_session_ids_capped_at_256`;
  - `events::tests::dispatcher_catches_up_after_restart`(写入→不派发→重建 dispatcher→消费者收到);
  - `events::tests::consumer_failure_does_not_block_others`;
  - `events::tests::retention_never_deletes_unconsumed`(含审计 #12 的三场景:缺 offset 行、新消费者、新 tenant);
  - `events::tests::oneshot_role_appends_but_never_dispatches`;
  - `events::tests::each_committing_batch_writes_event_row`(v2 #1:注入批间崩溃,断言无"已提交无事件"缺口);
  - `events::tests::resident_host_consumes_oneshot_appends_within_poll_interval`(v2 #2:跨进程唤醒集成测试,不发本地 notify,断言时间上界内被消费);
  - `events::tests::late_consumer_backfills_then_cuts_off`(v2 #13)。
- 压测冒烟:1 万行 outbox 启动 catch-up < 5s(本地基准,CI 放宽)。
- 契约:本篇不新增对外 method;`cargo test --workspace` 全绿。
