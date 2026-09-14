# Memory Q1–Q36：L1 Recent Snapshot 与双水位管线

## 1. 管线边界

L1 是租户级、低频、时间窗口优先的聚合 Snapshot。它消费成功 Session Memory 和必要的 canonical Conversation 证据，不替代 Phase 1。

```text
Conversation commit
  -> Session Memory Phase 1（现有事件驱动、只更新 SQLite）
  -> 到达 Recent 水位
  -> Recent candidate set
  -> content fingerprint
  -> reuse 或 ACP generation
  -> application admission
  -> SQLite Snapshot transaction
  -> Markdown projection request
  -> UI 读取 last-success
```

Session Memory 完成不得直接替换用户可见 Recent Snapshot。它只使下一个水位的 candidate/fingerprint 发生变化。

## 2. 时钟与水位

### 2.1 设置

- `recentWindowHours ∈ {24, 48, 72}`，默认 48。
- `recentWatermarks` 恰好两个不同的本地墙上时钟时间，默认 `02:00`、`14:00`。
- 保存时按本地时间升序规范化；调度时使用宿主当前本地时区。

### 2.2 目标计算

协调器每次启动、设置变化、定时唤醒和相关 Job 终态后执行：

1. 读取当前本地日期、时间和时区偏移。
2. 生成今天与前一天的四个水位候选。
3. 转换为 UTC instant，选择 `<= now` 的最大候选。
4. 若该目标已有 succeeded/reused Snapshot 或同 fingerprint 的 queued/running Job，则不重复入队。
5. 应用离线期间存在多个未执行水位时，只入队第 3 步的最新目标。

DST 规则：不存在的本地墙上时间顺延到该日期第一个有效 instant；歧义时间选择第一次出现的 instant。Snapshot 保存实际本地日期、时间和 offset，以便审计。

### 2.3 窗口

```text
window_end_utc   = target_watermark_utc
window_start_utc = target_watermark_utc - recentWindowHours
```

边界采用左闭右闭：`window_start_utc <= last_activity_at <= window_end_utc`。目标水位之后到达或活动的 Session 属于下一 Snapshot。

## 3. Candidate Set

候选 Session 必须同时满足：

- tenant 与 Job 一致；
- canonical Session 可见、未 missing；
- source enabled；
- execution origin 不是内部 Memory generation；
- Session 与 source 未被 Memory 设置排除；
- `last_activity_at` 落入窗口；
- 存在当前成功 Session Memory，或 canonical revision 相比 Session Memory 更新而能够在预算内先完成 Phase 1。

快照生成不得因为个别 Session 的 Phase 1 尚未成功而假装完整。处理规则：

- 能在本 Job 的前置协调阶段完成 Phase 1：等待其 Durable Job 终态后重新计算 candidate；
- Phase 1 failed、budget exhausted 或 evidence incomplete：Recent Job 终态为 `failed`/`MEMORY_COVERAGE_INCOMPLETE`，保留 last-success；
- Session 明确无可记忆内容且 Phase 1 为成功 skipped：计入 coverage，但不要求生成 Item。

Candidate Set 按 `project_key`、`last_activity_at`、Session ID 稳定排序。项目 key 继续遵守登记项目根 → Git worktree 根 → 规范化 cwd；无归属使用 `unassigned`。

## 4. 两类 Fingerprint

### 4.1 Target Fingerprint

用于 Job 幂等，包含：

```text
tenant_id
target_watermark_utc
window_hours
sorted candidate Session identities and revisions
prior active L1 item revisions
Skill asset ID/revision/content hash
Memory contract version
budget policy version
projection policy version
```

序列化使用字段名排序的 canonical JSON，再计算 SHA-256。

### 4.2 Content Fingerprint

用于无变化复用，不直接包含 target watermark；包含在该目标水位上计算出的：

```text
window_hours
eligible Session identities/revisions/fingerprints
eligible carry-over item revisions and remaining lifetime bucket
current L2/L3 revisions supplied to generation
Skill and policy versions
exclusion/source availability state
```

候选 Session 进入/退出窗口、carry-over 到期、终态变化、来源可用性变化、设置变化或 Skill 变化都会改变 content fingerprint。

### 4.3 Reuse

当 content fingerprint 与最近成功 Snapshot 一致：

1. 不调用 Agent。
2. 新建当前目标的 Snapshot，`publication_kind=reused`。
3. 项目与 Item membership 指向上一 Snapshot 的相同 revision。
4. `reused_from_snapshot_id` 指向直接来源。
5. `published_at` 更新；`content_generated_at` 保留原实际生成时间。
6. 推进 last-success 与目标水位。
7. reused Snapshot 不增加 L2/L3 晋升观察次数。

## 5. Work Order 构建

Recent Job 入队时固定不可变 Work Order。证据首包包括：

- 目标水位、窗口和项目集合；
- 每个 Session 的目标、结果、决定、验证、阻塞、待办、主题、revision 和短引用索引；
- 上一 Snapshot 中仍可能续接的 L1 Item：稳定 ID、状态、最后证据、首次/最后出现时间；
- 当前有效 L2/L3 的高密度摘要，用于避免把已有长期决定当作新事实重复提名；
- coverage、预算、可用工具和输出 Schema。

不包含：旧 Markdown、全量 Session JSON、Provider 调试元数据、完整工具日志、无关项目正文和完整 Prompt 日志。

## 6. Agent 输出准入

Agent 返回 `MemoryGenerationResultV2`，详见 `05-generation-skill-acp.md`。应用按以下顺序处理：

1. Schema、字符数、数组上限与枚举校验；
2. tenant、项目 key、Session 与引用范围校验；
3. reference membership 与 source revision 校验；
4. project summary 与 Item 去重；
5. `continuesItemId` 兼容性验证；
6. recommendation rank 唯一性和 0–3 限制；
7. promotion nomination 只记录候选，不直接修改层级；
8. coverage 必须覆盖 Work Order 中所有 candidate，或明确把成功 skipped Session 列为无事实；
9. secrets redaction 后再次执行字段上限和空值检查。

任一步失败，Job 失败且不提交 Snapshot。

## 7. L1 延续算法

### 7.1 可续接状态

只有 `active`、`blocked`、`waiting` 可由 Agent 通过 `continuesItemId` 续接。应用要求相同 tenant/project、兼容 category、合法证据延续。

### 7.2 生命周期

- `first_seen_at` 在 Item 创建时固定。
- `last_seen_at` 在有新有效证据的 generated Snapshot 更新。
- 可续接截止时间为 `first_seen_at + 7 days`；超过截止后旧 Item retired，即使 Agent 再次引用也创建新 Item 或明确新阶段。
- reused Snapshot 不延长 `last_seen_at`，也不刷新 7 天上限。
- 新证据可以改变 active/blocked/waiting 之间状态并形成新 revision。

### 7.3 终态

当 generated Snapshot 把 Item 更新为 `completed`、`verified`、`abandoned` 或 `superseded`：

- 该 revision 加入当前 Snapshot；
- Item 标记 `terminal_shown_snapshot_id=current`；
- 后续 Snapshot 不再自动携带该 Item；
- L2/L3 中已晋升的关联知识不受影响。

### 7.4 候选缺席

- 某个未完成 Item 在一个 generated Snapshot 中缺席，但其来源仍有效：保持审计记录，不进入当前 L1；不自动标记完成。
- reused Snapshot 不视为缺席。
- 来源删除、missing、source disabled 或被排除：未晋升 L1 在下一 generated Snapshot 退出，引用标 unavailable。

## 8. 项目输出

每个有候选 Session 的 project key 必须有一个 `RecentSnapshotProject`：

- `summary`：说明窗口内做了什么；无有效事实时使用结构化 `no_material_change=true`，正文为固定本地化文案，不让 Agent编造。
- `next_steps`：0–3 个 `recommendation_rank` Item，每个有至少一个有效 reference。
- `items`：决定、进展、研究、验证、阻塞、待办；每项有 title、summary、rationale、status、occurred_at 和 references。
- `source_session_count`：应用计算，不相信 Agent。

项目标题由应用的 Project Directory 投影得到。Agent 返回标题只作候选显示文本，不改变 project identity。

## 9. 事务与发布

成功生成的提交顺序在一个 SQLite 事务内完成：

1. 验证 ownership token、target fingerprint 和当前设置/Skill/source revision；
2. 插入/更新 Item 与 revision；
3. 插入 source references；
4. 插入 Snapshot、Project 与 membership；
5. 插入 promotion observations；
6. 更新 tenant Recent last-success 指针；
7. 将 Job 标记 succeeded/reused；
8. 提交事务；
9. 事务外入队 Markdown projection rebuild 并通知 TaskRuntime/UI。

Markdown 发布失败不撤销步骤 1–8。SQLite 事务失败时不写 Markdown。

## 10. 失败与恢复

| 错误 | retryable | 可见行为 |
|---|---:|---|
| Agent 临时退出/超时 | 是 | last-success + “更新未完成”，任务中心可重试 |
| Skill 无效 | 否，直到配置变化 | 不调用 Agent，设置页与任务中心显示配置错误 |
| Evidence coverage 不完整 | 是或需上游修复 | 不发布部分 Snapshot |
| 输出 Schema/引用非法 | 是，受最大重试限制 | 不发布；保留错误码与计数，不记录正文 |
| 预算耗尽 | 否，直到策略或输入变化 | 明确 budget exhausted，不等同无内容 |
| 提交前发现 stale | 否 | 丢弃旧结果，协调最新目标 |
| Markdown 发布失败 | 是 | Snapshot 仍成功；只重试投影 |

所有失败均保持页面 last-success，不向用户发起交互。
