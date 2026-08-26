# 0013: Conversation 采用 membership-first Question 与 Content Node 投影

> **状态**：已接受
> **决策日期**：2026-08-25
> **记录日期**：2026-08-26
> **取代**：Question 内容快照、物理 question index、grouping origin 与后端 Card 数组作为读取权威的早期实现
> **实现证据**：`src-tauri/migrations/202608250004_conversation_data_audit.sql`、`src-tauri/migrations/202608250005_rebuild_conversation_questions.sql`、`src-tauri/migrations/202608260001_repair_conversation_question_contract_release.sql`、提交 `7621dbe`、`1886947`、`1ee0141`
> **实现状态**：核心 contract 已落地；历史收口、协作式取消、性能验收和兼容删除仍按 `07-post-luna-audit.md` 追踪

## 背景

Conversation 早期把问题正文、回答/代码/命令快照、物理顺序和分组来源写入
`conversation_questions`，读取侧又同时暴露 Card 数组和旧的并行内容结构。该模型在
Question 合并/拆分、Shell Execution 一对多展示、搜索重建和历史修复时产生第二套身份
和内容权威。

本次迁移完成了主要消费者切换，但施工后审计确认 #13、#15、#16 尚未满足全部验收。
用户明确要求 Memory 后续整体重写；本轮不审计、不修改、不验收 Memory 功能。

## 决策

1. `conversation_question_turns` 与 `web_record_question_turns` 是 Question–Turn membership
   的唯一权威。Question 顺序由 membership 对应 Turn 的 `turn_index` 推导，分组来源只在
   membership 上保存。
2. `conversation_questions` 与 `web_record_questions` 只保存 tenant、session、稳定 ID、
   标题和时间元数据；正文、`question_index`、`grouping_origin` 和 Question 内容快照不再
   进入物理表。
3. 标准 Question Detail 读取模型是 `Question → QuestionTurn → Turn → Part → Content Node`。
   Content Node 带有 Question/Turn/Part/节点顺序 locator；一个 Part 可以投影多个节点。
4. Card 只保留为 UI 展示概念；后端 DTO、Engine 和 CLI 不以 Card 数组或 Card ID 作为领域
   身份。
5. Shell Execution 按一次原始执行保存一个 Part；逐条命令展示属于读取时 Content Node
   投影，不复制 Part。
6. 历史数据通过审计、dry-run、可验证备份、可选可靠来源全量重同步、批量安全修复、搜索
   重建和最终审计完成收口。无法证明来源的旧拆分执行只进入审计，不拼接伪造原始事实。

## 迁移与回滚

- `202608250004` 建立租户级审计问题表；问题使用稳定 fingerprint，便于重复审计和幂等
  修复。
- `202608250005` 通过 SQLite 受控表重建移除六个 Question 快照/推导字段，并原样复制
  当时的标题。该已发布 migration 曾被施工修改并引发 checksum 启动失败，现已恢复原始字节。
- `202608260001` 以新 migration 补建所需索引、从首个 Turn 回填空标题，并记录无法再逐行
  重建的保守 Question 快照依赖计数。
- `conversation.data.repair` 在 apply 前强制生成并验证数据库备份，支持 `dry_run`、
  `yes`、`resync` 与最终 verify；`conversation.data.rollback` 恢复备份后要求重启应用。
- 迁移后回滚依赖 apply 前备份，不尝试从已删除的 Question 正文快照重建事实。

## 后果

- 合并/拆分只改变 membership 与引用，源 Turn/Part 身份保持稳定。
- 搜索、导出、深链接和前端读取共享同一层级 DTO 与 Content Node locator。
- UI 可以扩展展示节点数量，但持久化 Part 数量仍按真实执行单元计算。
- Memory 的领域行为不在本决策范围；其后续整体重写应重新定义证据与 Question 消费契约。
- 本 ADR 记录目标 contract，不等同于最终验收通过；施工后未完成项以
  `agent-docs/feature-plans/conversation-semantic-projection-refactor/07-post-luna-audit.md` 为准。
