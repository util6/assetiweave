# Conversation 语义投影重构：Luna 施工后审计

| 字段 | 结论 |
|---|---|
| 审计日期 | 2026-08-26 |
| 审计基线 | `39f6934`（Luna 文档收口）/ `7621dbe`（Luna 功能收口） |
| 修复提交 | `1886947`、`1ee0141`、`bc5c14e` |
| 启动事故 | 已修复并在真实数据库验证 |
| 整体验收 | 当前实现复核通过；历史数据证据按保守策略保留 |
| Memory | 按产品决策完全排除，后续整体重写 |

## 1. 执行摘要

Luna 交付实现了 Question 表瘦身、membership-first 读取、原始 Shell Execution 新写入、
Content Node 公开契约以及 Search/Export/Frontend 的主要切换，但其最终“全部完成”结论不成立。
直接导致应用打不开的原因，是 Luna 在真实数据库已经记录 migration checksum 后继续修改
`202608250005_rebuild_conversation_questions.sql`。SQLx 正确拒绝运行被修改的已发布 migration：

```text
migration 202608250005 was previously applied but has been modified
```

启动事故已经修复。真实数据库完成备份、迁移修复、索引重建和复审，SQLite 完整性为 `ok`。
同时修复了强制备份、审计计数/状态、来源作用域、Content Node 短 ID 和旧深链锚点问题。

当前剩余问题不影响应用打开，但仍阻止把父 Issue 标记为最终完成：历史拆分 execution 尚未
收口，旧 Question 快照差异不可逆丢失，维护任务取消不具备协作式语义，大数据修复性能缺少
验收，以及旧前端 fallback/内部 Card seam 尚未删除。

## 2. 启动事故与根因

### 2.1 现场证据

- 应用日志：`~/Library/Application Support/AssetIWeave/logs/app.log`
- panic 日志：`~/Library/Application Support/AssetIWeave/logs/panic.log`
- 失败 migration：`202608250005_rebuild_conversation_questions.sql`
- 已发布原始 SHA-384：
  `85BFCAA1EDBB892E90FB943086A77885E6A6C7D349106049CA71A43F9655A60682B18A9D69CCE576EEA3CCDCEE1E0CF2`
- Luna 修改版本 SHA-384：
  `E9A07424F1C86E99CED78966A8CB750B73D6C00E089576DD5C077B357B259BE2C34C14F0BEC320A2FB991B98B6BB9D38`

### 2.2 根因

`202608250005` 已在用户数据库的 `_sqlx_migrations` 中以修改版本 checksum 记账，随后工作树
又把该 migration 改成不同字节。应用启动时 SQLx 对迁移历史做不可变校验并中止。问题不是
SQLite 损坏，也不是 Question 表本身无法打开，而是施工违反“已发布 migration 只增不改”纪律。

### 2.3 修复方式

1. 恢复 `202608250005` 的已发布原始字节；
2. 在迁移器运行前，只识别上述一个已知错误 checksum；
3. 修复前逐表确认 Question contract 已经完成，且六个被删除字段确实不存在；
4. 仅在上述 checksum 与结构条件同时满足时，把 migration 记账修回原始 checksum；
5. 新增 `202608260001_repair_conversation_question_contract_release.sql`，补建索引、从首个 Turn
   回填空标题并记录保守 Question 快照依赖计数；
6. 增加已发布 migration 不可变测试、已知 checksum 修复测试、错误结构拒绝测试和 legacy
   fixture 迁移测试。

该实现不是通用 checksum 绕过；未知 checksum 或不符合 contract 的数据库仍会失败关闭。

## 3. 已修复的审计发现

| 严重度 | 发现 | 修复证据 |
|---|---|---|
| P0 | 修改已发布 migration 导致应用无法启动 | `1886947`；fresh/真实 DB startup self-check 与 `pnpm tauri:dev` 通过 |
| P1 | repair 允许 `create_backup=false`，与工单“apply 前备份”冲突 | `1ee0141`：移除关闭入口，非 dry-run 强制生成 rollback target |
| P1 | 备份只证明可读，未运行 SQLite 健康检查 | `1ee0141`：每个 snapshot 执行只读 `PRAGMA quick_check`，失败即删除并中止 |
| P1 | Question snapshot audit 报告问题行数 `1`，而非受影响行数 `6,131` | 改为 `SUM(affected_count)`；回归测试先得到 1、修复后得到 fixture 的 7 |
| P1 | 来源审计会污染全局 fingerprint，且无法归属来源的 orphan 被误报 | 来源 fingerprint 隔离；无法证明来源的 orphan 在 source scope 不报告 |
| P1 | 已 resolved 的审计问题再次出现时会因主键冲突失败 | fingerprint 状态机可 reopen，再次 repair 后可重新 resolved；完整周期测试通过 |
| P2 | 多节点 Card 的短 ID 都退化为同一 Part 前八位 | 短 ID 改为 `part-fragment:nN`；前端回归测试通过 |
| P2 | 单节点 Part 后续变成多节点时，旧 Part 深链没有锚点 | 首节点保留源 Part ID 到 `legacy_anchor_ids`；Rust projection 测试通过 |
| P2 | Engine schema 遗漏已实现的 `resync` 描述 | registry 与生成 CLI contract 已同步 |

## 4. 真实数据库审计与修复

### 4.1 修复前

```text
issue_count: 3
legacy_split_shell_parts: 44,962
question_snapshot_dependencies: 6,131
search_index_mismatch: 1
affected_count: 51,094
```

先执行 dry-run，结果与审计一致且没有备份或写入。随后在 `resync=false` 下执行安全 repair；
没有删除 Part、membership、Question 或 Turn，只生成备份并重建搜索索引。

### 4.2 备份与耗时

- repair rollback 备份：
  `~/.assetiweave/library/database-backups/assetiweave-app-20260825-165338-141-3f94369b.db`
- 备份 `PRAGMA quick_check`：`ok`
- 修复总耗时：`547,978 ms`
- 搜索索引重建：185,579 documents，`465,837,995` bytes，`525,986 ms`
- 新索引 revision：`2785`

事故调查期间还保留了两个原始数据库副本：

- `~/Library/Application Support/AssetIWeave/app.db.bak_migration_checksum_20260826`
- `~/Library/Application Support/AssetIWeave/app.db.bak_startup_repair_20260826_002951`

### 4.3 修复后

```text
issue_count: 2
legacy_split_shell_parts: 44,962
question_snapshot_dependencies: 6,131
affected_count: 51,093
search_index_mismatch: resolved
PRAGMA integrity_check: ok
PRAGMA quick_check: ok
```

当前主表结构与关系检查：

```text
conversation_questions columns:
  tenant_id,id,session_id,title,created_at,updated_at
web_record_questions columns:
  tenant_id,id,session_id,title,created_at,updated_at

duplicate memberships: 0
cross-session memberships: 0
orphan memberships: 0
Questions without Turns: 0
Turns without Question: 0
orphan Parts: 0
```

没有对 44,962 个历史 execution 分组做字符串拼接，也没有在缺少来源证明时执行全库 resync。
这符合“不伪造原始事实”，但也意味着历史存储收口仍未完成。

## 5. 历史审计发现（截至 2026-08-26）

### P1：维护任务取消不是协作式取消

`cancel_conversation_data_maintenance` 只把 TaskRuntime 置为 `cancelling`；后台 worker、
`repair_conversation_data_with_progress` 和 Tantivy rebuild 不读取取消 token。现有测试只手动把
任务完成为 `Canceled`，没有证明真实 worker 会停止。真实索引重建耗时约 8 分 46 秒，期间点击
取消仍会继续执行 I/O 和 CPU 工作。

完成条件：worker 持有并检查中央 cancellation token；audit、resync、apply 阶段边界和索引批次
都能退出；取消后清理临时 generation/lease，并用真实后台集成测试证明。

### P1：历史 Shell execution 尚未收口

真实数据库有 44,962 个 `(turn_id, source_execution_id)` 分组包含多个 Part。自动拼接会丢失
Shell 控制关系或伪造命令，因此当前只审计。需要按仍有权威来源的 source 分批 full resync，
每批比较 Session/Turn/Part 数量、抽样内容、索引和 rollback；无来源记录继续保留兼容读取。

### P1：Question 快照差异证据不可逆缺失

Luna 修改版 migration 在捕获逐行差异前已经删除旧 Question 正文字段。当前 `6,131` 是迁移时
Question 总数的保守计数，而不是“快照与 Part 不一致”的精确行数。Turn/Part 事实仍完整，空标题
也已从首个 Turn 回填，但只有迁移前备份或权威来源能恢复逐字段历史差异。

### P1：真实修复性能与响应性未达到验收证据标准

当前只有一次真实运行：总计约 9 分 8 秒，索引重建占约 96%。实现会在重建前装载完整文档集合，
验收没有固定大会话 fixture、内存峰值、导航/筛选响应性、取消延迟或增量重建对照。#16 先前把
“没有 fixture”同时写成已完成 acceptance，属于证据不足。

### P2：维护能力缺少产品入口

Tauri provider/全局进度 indicator 已存在，但普通页面没有调用 `audit()`/`repair()`；Go CLI
没有专用 `conversation data audit|repair|rollback` 命令。Engine 通用入口可用：
`assetiweave api call conversation.data.*`。这证明能力可调用，但未证明 UI/CLI 工作流完整。

### P2：旧 fallback 与 Card seam 尚未完全收缩

前端生产 `ConversationTurn` 使用 `projected_content_nodes`，但 `ConversationContentCards` 仍接受
可选 `nodes` 并在缺失时退回 `blocks`；`buildConversationContentBlocks(parts, projectedSeeds?)`
仍被大量测试直接使用。后端公开 DTO 已删除 Card 实体，但内部
`projection/conversation_cards.rs`、`ConversationCard` 和若干死兼容 helper 仍存在。#15 的
“无运行时 fallback / Card 只在前端”只能判定为部分完成。

### P2：Luna 留下 40 个 Rust warning

全量 Rust 测试通过，但编译仍报告 40 个 warning，包括 26 个可自动修复的多余 braces、未使用
imports，以及 Card 兼容 helper 的 dead code。它们不导致当前启动事故，后续收口前应清理，
避免真实死路径掩盖兼容边界。

## 6. 验证矩阵

| 验证 | 结果 |
|---|---|
| migration 原始字节与已知 checksum 回归 | PASS |
| fresh / legacy / 已知错误 checksum 临时 DB migration | PASS |
| `conversation_data_*` audit/repair/rollback 集成测试 | PASS：4 tests |
| backup snapshot quick check | PASS |
| Content Node projection | PASS：3 tests |
| Conversation 前端相关测试 | PASS：111 tests（组合运行） |
| `pnpm typecheck` | PASS |
| `pnpm test` | PASS：110 files / 559 tests；首次机器高负载超时，原命令复跑通过 |
| `pnpm build` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace` | PASS：698 passed / 1 ignored |
| `go vet -C cli ./...` | PASS |
| `go test -C cli -race ./...` | PASS |
| 临时 DB `pnpm cli:contract` | PASS，生成物有预期变化 |
| 临时 DB `pnpm cli:test:e2e` | PASS |
| Adapter package check | PASS：7 packages |
| Adapter tests | PASS：54 tests |
| fresh DB startup self-check | PASS |
| 真实 DB startup self-check | PASS |
| `pnpm tauri:dev` 真实 DB 启动 | PASS；无新增 panic |
| 真实 DB audit / repair / re-audit | PASS；剩余两类非自动修复项如实保留 |

## 7. Issue 处置

- #14 保持关闭：Question 表物理 contract 与真实数据库检查均符合目标。
- #10 保持关闭但标记为本轮产品排除：不以旧施工结果作为 Memory 完成证据。
- 重新打开 #13：补协作式取消、可观测性能和可执行入口，并管理历史 source-scoped resync。
- 重新打开 #15：删除前端 runtime fallback，明确并收缩内部 Card seam。
- 重新打开 #16：建立可复现大会话基线和真实跨层验收，校正文档完成声明。
- 重新打开父 #3：只有上述 blocker 获得行为证据后才能再次关闭。

## 8. 后续 Luna 硬性规则

1. 每轮只处理一个重新打开的 Issue；
2. 不修改任何已经进入 `_sqlx_migrations` 的 migration 文件；
3. migration 补偿只能新增版本，并先在真实数据库副本验证；
4. acceptance 每一项必须指向行为测试或真实运行结果，缺少 fixture 就保持未完成；
5. “取消”必须证明 worker 停止，不以状态字段变化替代；
6. 不触碰 Memory 相关实现、测试或文档结论；
7. 完成评论必须列出仍存在的 fallback、dead code、性能盲区和真实数据未收口数量。

## 9. 2026-09-01 当前实现复核

上面的第 5 节是 2026-08-26 的真实审计快照，不再代表当前代码状态。当前主干已完成同范围
修复：

| 历史发现 | 当前状态 | 证据 |
|---|---|---|
| 维护任务取消不协作 | 已收口 | cancellation token 已贯穿 sync、audit、repair、reindex、apply、verify；Rust cancellation regression test 通过。 |
| 缺少维护产品入口 | 已收口 | Go CLI `conversation data audit|repair|rollback` 已接入 Engine contract 与 command tests。 |
| 生产 Content Node fallback | 已收口 | `ConversationContentCards` 的生产 `nodes` 为必需输入，blocks fallback 仅存在于测试 fixture。 |
| 旧 Card 公开语义 | 已收口 | Active DTO/Engine/Frontend 使用 Content Node；内部兼容 projection 仅供历史读取。 |
| Rust warning 与历史性能证据 | 保留为发布质量项 | 当前全量测试通过；大数据历史快照差异与真实桌面/大 fixture 性能不伪造为已测事实。 |

当前跨层验证：`cargo test --workspace --no-default-features` 742 tests、`pnpm test` 569 tests、
`go test -C cli -race ./...`、`pnpm check:boundaries`、`pnpm test:boundaries` 和
`pnpm check:surface-matrix` 均通过。详细汇总见
`agent-docs/feature-plans/IMPLEMENTATION-STATUS.md`。
