# T06：实现来源失效与派生重建传播

## Outcome

Session 修订、删除、来源缺失、项目目录变化、排除设置变化和 Memory contract 升级会使旧派生结果不可被当前读路径使用，并幂等推进 Session→Project→Global、Recent、索引与文档重建。

## Blocked by

T03、T04。需要可靠 Job 生命周期和 Project aggregation。

## Read

- Contracts：C-D01、C-D02、C-A02–C-A05、C-P04、C-P07、C-P08、C-M01–C-M04、C-R04、C-S01、C-S03。
- Seams：S02–S05、S08、S16；Tests TS01、TS02、TS04。
- Gates：G0、G1、G2、G6、G7。

## Authority changed

新增派生 freshness/invalidation 状态与级联水位；删除事实仍来自 Conversation。

## Red test first

为 Session v1 生成全层结果，再依次更新到 v2、删除、标记来源缺失、改变项目目录和升级 contract version：旧 Session 不进入当前 Project/Recent/Search；新旧项目各自得到正确 dirty watermark；重复 invalidation 不增加重复工作。

## Execution steps

1. 定义 freshness 判定与 source fingerprint 版本。完成标准：当前/失效/生成中/last-success 状态可由公开 DTO 区分。
2. 将 Conversation 修订、删除、缺失和项目变更接入 Outbox consumer。完成标准：先 durable enqueue/invalidation commit 后 ack。
3. 实现 Session→Project→Global 级联与旧项目移除。完成标准：迁目录同时使旧 scope dirty 并将新 scope 入队。
4. 同步 Recent Event、semantic index 和 Markdown projection 失效。完成标准：公开读路径不会返回已删除来源，投影可重建。
5. 接入排除设置与 contract/prompt version 升级。完成标准：排除不删 Conversation；解除排除可重新入队。

## Acceptance

- [ ] 六类来源/合同变化都产生正确级联。
- [ ] 旧版本保留审计但不进入当前读结果。
- [ ] 删除/缺失来源从 Recent、Recall index 和 Context 移除。
- [ ] 项目迁移同时更新旧、新项目。
- [ ] 重复事件与重复扫描幂等。
- [ ] last-success 只在其来源仍有效时可读。

## Non-goals

Recall Agent、Recent UI、usage/retention、legacy 数据导入。

## Ticket-specific stop

如果删除传播依赖物理删除全部历史或无法区分审计版本与当前版本，停止并报告。
