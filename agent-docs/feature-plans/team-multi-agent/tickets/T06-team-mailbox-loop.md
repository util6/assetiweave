# T06：建立 TeamMailbox 协作闭环

## Outcome

Teammate 的任务结果通过持久 TeamMailboxMessage 交给 Leader；Leader 消费结果并在自己的 Provider Session 中生成用户可见总结，任务板与 mailbox 共享 SQLite Authority。

## Blocked by

T05。

## Read

- Contracts：C-D01、C-D03、C-D04、C-H01、C-T01、C-A01、C-A02、C-S01。
- Seams：S01、S02、S07、S09、S10、S12–S15；Tests TS01、TS04、TS05。
- Gates：G0–G4。

## Red test first

单任务完成后，AppService 必须持久化一条归 Leader 的 unread mailbox 事实；Leader 消费并回复后消息只被确认一次，重复协调不会再次向 Leader 发送相同结果。

## Execution steps

1. 建立 TeamMailboxMessage identity、sender/recipient、run/task 关联、read/ack 和幂等键。完成标准：重复写入和重复消费不产生第二条逻辑消息。
2. 在单任务 terminal transition 中原子提交结果 mailbox 事实。完成标准：任务状态与消息不会一半成功。
3. 让协调器把 Leader 未读消息作为新 live input 发送到 Leader context。完成标准：Leader 回复进入 Provider Session/临时时间线，mailbox 标记已消费。
4. 接入任务板和 Leader timeline 的结果状态。完成标准：用户能看到任务事实和 Leader 总结，但 Team 表不保存主聊天 transcript。

## Acceptance

- [ ] TeamTask terminal 与结果 mailbox 提交保持一致。
- [ ] Leader 只消费属于自己的未读消息。
- [ ] 重复 finish、poll 或 coordinator 调用不会重复总结。
- [ ] Leader 总结保存在 Leader Provider Session，不复制到 Team/Conversation transcript。
- [ ] mailbox 读写不包含日志中的正文或凭据。

## Non-goals

MCP transport、CLI fallback、多 Teammate 调度、restart outbox 和长期 mailbox retention policy。

## Ticket-specific stop

如果必须把 mailbox 当作 UI 聊天记录，或用前端本地状态作为 unread Authority，停止并报告。

