# Memory Q1–Q36：领域模型与 SQLite 存储规范

## 1. 领域对象

### Session Memory

现有 Phase 1 派生事实。身份绑定 tenant、Session、source revision/fingerprint 与 contract。它可以事件驱动更新，但不等同于用户可见 Recent Snapshot。

### Memory Item

跨 Snapshot 延续并可晋升的最小语义单元。一个 Item 有稳定 ID，正文变化形成 revision。

```text
MemoryItem
  id
  tenant_id
  layer: l1 | l2 | l3
  project_key: nullable
  current_revision_id
  lifecycle: current | superseded | retired
  first_seen_at
  last_seen_at
  created_at
  updated_at
```

```text
MemoryItemRevision
  id
  item_id
  revision_number
  category: progress | decision | research | verification | blocker | follow_up
  status: active | blocked | waiting | completed | verified | abandoned | superseded
  title
  summary
  rationale
  recommendation_rank: null | 1 | 2 | 3
  promotion_nomination: none | project_decision | project_constraint |
                        recurring_blocker | recurring_todo | research_conclusion |
                        global_rule | cross_project_pattern
  occurred_at
  evidence_fingerprint
  generated_by_snapshot_id
  supersedes_revision_id: nullable
  created_at
```

`recommendation_rank` 非空表示该条目是项目的下一步建议。每个 Snapshot/项目最多出现 1、2、3 各一次。

### Source Reference

引用是 locator 的不可变副本，不以 Conversation 外键级联删除。

```text
MemoryItemSourceReference
  id
  item_revision_id
  record_kind: session
  source_id
  session_id
  question_id: nullable
  turn_id: nullable
  part_id: nullable
  node_id: nullable
  node_order: nullable
  source_revision
  availability: available | unavailable
  unavailable_reason: null | deleted | missing | excluded | source_disabled
  unavailable_at: nullable
  created_at
```

引用有效性影响跳转和新晋升，不删除已经晋升的知识。跨 tenant、错误 membership 或越界 node 在写入前拒绝。

### Recent Snapshot

Snapshot 是目标水位的完整、已发布 L1 视图。

```text
RecentMemorySnapshot
  id
  tenant_id
  sequence
  target_watermark_utc
  local_watermark_date
  local_watermark_time
  timezone_offset_minutes
  window_hours: 24 | 48 | 72
  window_start_utc
  window_end_utc
  publication_kind: generated | reused
  reused_from_snapshot_id: nullable
  target_fingerprint
  content_fingerprint
  generation_skill_asset_id
  generation_skill_revision
  generation_skill_content_hash
  contract_version
  budget_policy_version
  projection_policy_version
  content_generated_at
  published_at
```

只有完整成功或明确复用的 Snapshot 才进入该表。失败尝试由 Durable Job 表记录，不创建可见 Snapshot。

### Snapshot Project 与成员关系

```text
RecentSnapshotProject
  snapshot_id
  project_key
  project_title
  project_path: nullable
  summary
  latest_activity_at
  source_session_count
  sort_order
```

```text
RecentSnapshotItem
  snapshot_id
  project_key
  item_id
  item_revision_id
  display_date
  sort_order
```

同一 Item revision 在一个 Snapshot 中至多出现一次。`project_key = "unassigned"` 是保留值；真实项目 key 不得使用该值。

### Promotion Observation

```text
MemoryPromotionObservation
  tenant_id
  item_id
  item_revision_id
  snapshot_id
  nomination
  evidence_fingerprint
  project_key
  observed_at
```

只有 `publication_kind=generated` 且证据 fingerprint 相比上一观察有有效变化时增加晋升观察。reused Snapshot 不增加次数，也不打断连续性。

## 2. 身份与去重

- ID 使用仓库统一稳定 ID 生成器；数据库主键始终包含 tenant。
- Agent 可以返回 `continuesItemId`，但应用只在下列条件全部满足时复用 Item：tenant 相同、project key 相同、layer 为 L1、category 兼容、引用与上一 revision 至少有一个合法重叠或明确的新证据延续同一事项。
- Agent 未提供或验证失败时创建新 Item；应用不得通过模糊标题相似度自动合并长期事实。
- 单次输出内的确定性重复键为 `project_key + category + normalized title + sorted source reference keys`。重复项在准入前合并或拒绝，不创建两个 Item。
- revision number 在同一 Item 内从 1 单调递增；提交使用事务内唯一约束防止并发重复。
- `supersedes_revision_id` 必须属于同一 tenant。取代另一个 Item 时，旧 Item lifecycle 变为 superseded，并通过单独 relation 保存 `old_item_id -> new_item_id`。

## 3. 逻辑表与约束

实现使用追加 migration 建立下列逻辑表；实际 SQL 名必须保持清晰的一一对应关系：

1. `memory_items`
2. `memory_item_revisions`
3. `memory_item_source_references`
4. `memory_item_supersessions`
5. `recent_memory_snapshots`
6. `recent_memory_snapshot_projects`
7. `recent_memory_snapshot_items`
8. `memory_promotion_observations`
9. `recent_memory_jobs`

硬约束：

- 所有表以 `(tenant_id, id)` 或包含 tenant 的组合键为主键/唯一键。
- Snapshot 对 `(tenant_id, target_watermark_utc, window_hours, contract_version)` 唯一。
- Job 对 `(tenant_id, target_watermark_utc, window_hours, target_fingerprint)` 唯一。
- Project row 对 `(tenant_id, snapshot_id, project_key)` 唯一。
- Item membership 对 `(tenant_id, snapshot_id, item_id)` 唯一。
- source reference 对 `(tenant_id, item_revision_id, reference_key)` 唯一。
- `window_hours`、layer、status、category、availability、publication kind 使用 CHECK 约束。
- 内容字段设置上限并在进入 SQL 前验证；超限是输出合同失败，不在数据库层静默截断。
- Source Reference 不对 Conversation 表声明 `ON DELETE CASCADE`；引用可用性由失效协调器更新。
- Snapshot、Project、Item revision、引用和 last-success 指针在一个事务中提交。

## 4. Job 模型

`recent_memory_jobs` 复用现有 Durable Job 语义：

```text
status: queued | running | succeeded | reused | failed | canceled | stale
ownership_token
lease_expires_at
heartbeat_at
attempt_count
retry_count
retry_at
target_watermark_utc
window_hours
target_fingerprint
content_fingerprint
work_order_json
last_error_code
last_error_message
started_at
finished_at
created_at
updated_at
```

- `stale` 表示结果在提交前发现 source/Skill/contract/target 已变化；不允许重试原结果，只能协调新目标。
- `reused` 是成功终态，对应一个 `publication_kind=reused` Snapshot。
- retry 只复用同一不可变 Work Order；设置或 Skill 已改变时创建新目标而不是篡改旧 Job。

## 5. Settings Schema

统一设置中的 `memory` 扩展为：

```json
{
  "generationEnabled": true,
  "usageEnabled": true,
  "recentWindowHours": 48,
  "recentWatermarks": ["02:00", "14:00"],
  "generationSkillAssetId": null,
  "excludedSessionIds": [],
  "excludedSourceIds": []
}
```

- `generationSkillAssetId = null` 表示使用内置默认模板。
- 水位格式固定 `HH:mm`、24 小时制、零填充；数组长度必须为 2、值不同，规范化后升序保存。
- 设置 schema version 追加升级；未知字段继续 round-trip。
- 保存设置只验证并持久化，不同步等待 Agent。

## 6. 状态机

### Item lifecycle

```text
new -> current -> superseded
               -> retired
superseded/retired 为终态；历史 revision 只读
```

L1 超过续接上限后为 retired。L2/L3 不因来源不可用自动 retired。

### Source reference

```text
available -> unavailable
unavailable 不自动恢复
```

同一来源重新出现且 identity/revision 可证明为同一事实时，新 revision 创建新的 available reference；不原地反转历史引用。

### Snapshot Job

```text
queued -> running -> succeeded
                  -> reused
                  -> failed -> queued (显式 retry，同 Work Order)
                  -> canceled
                  -> stale
```

只有 `succeeded` 与 `reused` 可以推进可见目标水位。

## 7. 迁移起点与兼容

- 现有 `session_memories` 与 source references 继续作为 Phase 1 输入，不进行破坏性重建。
- 现有 `recent_memory_events` 在 expand 阶段继续服务旧 API；首个新 Snapshot 可从成功 Session Memory 和当前 Conversation 构建，不能把旧 Markdown 解析为 Authority。
- 现有 Project/Global 最新成功内容在切换前保持 last-success。首次 L2/L3 backfill 读取 SQLite 中的成功来源集合和内容，经新合同重新生成结构化 Item。
- 现有 Recipe/Work Order 只用于历史审计和完成中的旧任务。新 contract version 的 Job 必须使用 Skill binding。
- 旧表在 contract 阶段可以保留历史数据，但不能再被当前读路径或生成路径依赖；删除旧表需要独立 migration 和实测备份策略。
