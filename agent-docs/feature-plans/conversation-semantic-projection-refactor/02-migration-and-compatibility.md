# Conversation 重构：迁移与兼容策略

## 1. 总体策略

采用 expand → migrate → contract，禁止直接删除字段后再追逐消费者错误。

### Expand

1. 强化现有 `conversation_question_turns`；
2. 建立 Question Detail 与 Content Node 新读取 seam；
3. 让新写入保存原始 Shell Execution；
4. 在明确边界内提供只读兼容形状。

### Migrate

1. 切换 reconciliation、Search、Memory、Export、前端、Engine 和 CLI；
2. 重建可推导索引；
3. 审计历史 membership、Question 快照、旧拆分 Shell Part 与消费者引用；
4. 在备份和 dry-run 后修复或从可靠来源重新同步。

### Contract

1. 重建并瘦身 `conversation_questions`；
2. 删除 Question 内容、推导顺序和分组来源字段；
3. 删除旧 Card DTO、平行数组和运行时 fallback；
4. 运行完整回归、性能和文档验收。

## 2. Migration 纪律

- 只新增 migration，不修改已发布 migration 或 checksum；
- SQLite 删除列优先使用受控表重建，并验证索引、触发器、外键和数据数量；
- 迁移测试至少覆盖旧 schema、空库、已部分迁移库和代表性历史库；
- 所有可能修改本地记录的测试使用临时 `ASSETIWEAVE_DB_PATH`；
- Question 表的物理瘦身只在所有生产消费者停止读写旧字段后执行；
- 生成契约必须用 `pnpm cli:contract` 重建，不手工编辑。

## 3. 历史数据分类

| 类别 | 处理 |
|---|---|
| membership 完整且一致 | 回填顺序、来源和关系时间后保留 |
| membership 重复或跨 session | dry-run 报告，按可证明事实修复，否则进入人工审计 |
| Question 内容快照与 Part 一致 | 以 Part 为权威，重建索引后丢弃快照 |
| Question 内容快照与 Part 不一致 | 报告差异；有来源时重同步，无来源时保留事实并标记审计 |
| Shell Execution 被历史脚本拆成多个 Part | 有可靠来源时按新 adapter version 重同步 |
| 只有拆分 Part、无可靠来源 | 保持兼容读取，不通过字符串拼接伪造原始 execution |
| evidence 可由 Turn/Part 唯一定位 | 重映射到当前 Question 和新 locator |
| evidence 不能唯一定位 | 进入显式审计状态，不静默猜测 |

## 4. 后台修复工作流

历史修复属于长运行工作，必须遵循仓库后台任务约束：

1. `start` 快速返回 task snapshot；
2. worker 使用独立服务/数据库连接，不在阻塞 I/O 时持有全局 app lock；
3. 阶段至少包括 audit、backup、apply/resync、reindex、verify；
4. 支持 dry-run，报告拟变更、不可自动修复项和影响范围；
5. apply 前建立可验证备份，并记录回滚入口；
6. 对输入去重、共享读取只加载一次、批量后统一刷新；
7. 通过事件更新进度，前端 provider 以轮询补偿丢失事件；
8. 只禁用冲突操作，浏览、筛选、设置和无关 CRUD 保持可用；
9. 关闭应用时检测运行中任务并进入现有关闭保护；
10. 失败和取消必须留下可解释终态，不留下半写 membership 或孤立索引。

## 5. 双读和兼容边界

兼容代码只能存在于明确的 adapter/DTO seam：

- 新领域 workflow 不读取 Question 内容字段；
- 新前端页面不在多个旧 DTO 之间猜测；
- 旧拆分 Shell Part 可由兼容 projector 展示，但不能反向写回伪造的原始 execution；
- Card alias 不得进入新的 Search、Memory、Export 或 Engine 契约；
- 每个兼容入口都必须有删除它的后续 blocker，最终由 #15 清除。

## 6. 回滚原则

- schema contract 前保留能够运行旧读取路径的回滚点；
- schema contract 后的回滚依赖迁移前备份，而不是尝试重新生成已删除的正文快照；
- raw Session–Turn–Part 事实不得因回滚策略而被修改；
- 索引、投影和 Question membership 可以从备份或权威事实重建；
- 每张涉及迁移的 Issue 完成说明必须列出 backup、verify 和 rollback 证据。
