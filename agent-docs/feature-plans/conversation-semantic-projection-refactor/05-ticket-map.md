# Conversation 重构：Ticket Map

父 Issue：[重构 Conversation 语义分组与内容投影，保留原始 Shell Execution #3](https://github.com/util6/assetiweave/issues/3)

所有子 Issue 均使用 `ready-for-agent` 标签、指定 Luna 执行，并已建立 GitHub 原生 sub-issue 与 blocker。

| 顺序 | Issue | Blocked by |
|---:|---|---|
| 1 | [#4 强化现有 Question–Turn membership 并建立 Question 投影 seam](https://github.com/util6/assetiweave/issues/4) | — |
| 2 | [#5 扩展结构化 Content Node 读取契约](https://github.com/util6/assetiweave/issues/5) | — |
| 3 | [#6 实现确定性 Question reconciliation 与 membership-first merge/split](https://github.com/util6/assetiweave/issues/6) | #4 |
| 4 | [#7 为 Codex 保留原始 Shell Execution 并投影多个命令节点](https://github.com/util6/assetiweave/issues/7) | #5 |
| 5 | [#8 将原始 Shell Execution 模型推广到其他官方 Adapter](https://github.com/util6/assetiweave/issues/8) | #7 |
| 6 | [#9 迁移 Question FTS、Search、Block Locator 与深链接](https://github.com/util6/assetiweave/issues/9) | #4、#5、#6、#7 |
| 7 | [#10 迁移 Memory evidence 与 Question ID 重映射](https://github.com/util6/assetiweave/issues/10) | #6、#9 |
| 8 | [#11 迁移 Export 与消费者专用载荷](https://github.com/util6/assetiweave/issues/11) | #4、#5、#6、#7 |
| 9 | [#12 前端切换到唯一的层级 Conversation 读取模型](https://github.com/util6/assetiweave/issues/12) | #4、#5、#6、#7、#9 |
| 10 | [#13 实现历史数据审计、后台修复与重新同步](https://github.com/util6/assetiweave/issues/13) | #6、#8、#9、#10、#11 |
| 11 | [#14 重建并瘦身 conversation_questions 表](https://github.com/util6/assetiweave/issues/14) | #6、#9、#10、#11、#12、#13 |
| 12 | [#15 收缩 Card DTO 与旧并行数组契约](https://github.com/util6/assetiweave/issues/15) | #8、#9、#11、#12、#13、#14 |
| 13 | [#16 完成集成验收、性能验证和架构文档收口](https://github.com/util6/assetiweave/issues/16) | #14、#15 |

## 可并行 frontier

仅在不同分支/worktree 中执行，且每张 Issue 仍由单个 Luna 上下文负责：

1. 初始：#4 与 #5；
2. #4 完成后：#6；#5 完成后：#7；
3. #7 完成后：#8；满足各自 blocker 后可推进 #9、#11、#12；
4. #6 与 #9 完成后：#10；
5. 消费者迁移完成后：#13；
6. #14 → #15 → #16 按 blocker 收口。

调度器每次以 GitHub 原生 blocker 的实时状态为准，不根据本表猜测已完成状态。
