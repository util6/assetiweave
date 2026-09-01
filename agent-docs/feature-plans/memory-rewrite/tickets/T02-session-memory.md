# T02：从 Conversation commit 生成 Session Memory

## Outcome

合格 Session 在 `ConversationSourceCommitted` 后先获得 durable Phase 1 Job，再由 Fake AgentExecutor 生成一个 revision-bound Session Memory、source references 与六类 Recent Event。

## Blocked by

T01。需要稳定 project directory 与 Recent source slice。

## Read

- Contracts：C-D01、C-D02、C-D04、C-D05、C-A01、C-A02、C-A04、C-P01–C-P05、C-M01、C-P09、C-S02、C-S03。
- Seams：S01–S06；Tests TS01、TS02。
- Gates：G0、G1、G2、G6、G7。

## Authority changed

新增 Session Memory、Recent Event、source reference 和 Phase 1 Job 的 SQLite authority；TaskRuntime 不保存这些事实。

## Red test first

提交一个已完成 Session，dispatch Outbox，再运行公开 worker：存在一个 queued/running Job 和一个成功 Session Memory。重放相同 event/revision 后 Job、Memory、Event 数量不增加；未完成且 idle 29:59 的 Session 不运行，30:00 后可运行。

## Execution steps

1. 用追加 migration 建立最小 Session Memory/Recent Event/source reference/Job schema 和唯一 fingerprint。完成标准：tenant、revision 与 contract version 参与唯一性，数据库重开可读。
2. 实现 AppService/repository durable enqueue 与稳定 Session gate。完成标准：Consumer 在 Job commit 后才推进 offset，大批事件可由 sync delta 恢复 Session。
3. 实现最小 Phase 1 worker，注入 Fake AgentExecutor、可控时钟和受限 action。完成标准：同 Session 不并发，模型输入先 redaction，非法/空输出产生安全失败。
4. 校验并持久化结构化 Session Memory、六类事件和 locator。完成标准：重复/非法 locator 去重或拒绝，回答正文和日志不暴露秘密。
5. 连接 Project resolver 与 Recent read model。完成标准：成功事件可被 T01 query 读取，Conversation 表未改变。

## Acceptance

- [ ] Outbox durable enqueue 在 offset 推进之前完成。
- [ ] 明确完成立即可运行；无完成信号需 idle 30 分钟。
- [ ] 相同 revision/event 重放完全幂等。
- [ ] Session Memory 包含 Issue #20 规定字段与可核实 locator。
- [ ] Recent Event 只允许六个类别。
- [ ] 模型输入输出经过 redaction，日志不含正文与 prompt。
- [ ] 测试只使用临时 DB、可控时钟和 Fake Agent。

## Non-goals

lease/retry/restart 完整调度、Project/Global consolidation、Markdown 文档、UI、Recall。

## Ticket-specific stop

如果 Consumer 必须直接 await Agent 或先 ack 后写 Job，停止并报告；不以页面触发补偿。
