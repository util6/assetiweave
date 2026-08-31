# T11：建立可重启 Team 协调器

## Outcome

确认后的 TeamRun、任务 terminal 和未读 mailbox 通过 durable outbox 唤醒常驻协调器；应用重启、重复投递或中途终止后继续处理而不重复 dispatch。

## Blocked by

T09、T10。

## Read

- Contracts：C-A01–C-A04、C-P03、C-P05、C-S01。
- Seams：S01、S07、S08、S09、S10；Tests TS01、TS06。
- Gates：G0、G1、G3、G4。

## Red test first

在事务中确认 TeamRun 并写 outbox，关闭首个 AppRuntime 后重开：同一 TeamTask 最终只产生一个可观察 execution claim；重复投递同一事件不产生第二次 dispatch 或 mailbox 消费。

## Execution steps

1. 定义 Team committed facts 和稳定 idempotency keys。完成标准：业务状态与 outbox 在同一事务提交。
2. 建立 Team coordinator consumer 并注册到 ResidentHost。完成标准：consumer 只调用 AppService/Team workflow，不持有全局 app lock 执行外部 I/O。
3. 实现 startup reconciliation。完成标准：扫描 durable non-terminal run/task/binding 后只补未完成工作。
4. 收敛 claim、dispatch、terminal、mailbox consume 的重复执行。完成标准：crash points 和重复事件具有一次可观察结果。
5. 增加 bounded shutdown。完成标准：停止接收新工作、等待/取消活动任务、报告剩余 durable work。

## Acceptance

- [ ] Team mutation 与 wake-up event 原子提交。
- [ ] ResidentHost 启动后处理已存在 outbox backlog。
- [ ] 重复 event/poll/reconcile 不重复 dispatch。
- [ ] 中断在 claim/execute/finish 各阶段后均可重开收敛。
- [ ] coordinator 不持有全局 app lock 等待 Agent/网络。
- [ ] shutdown bounded，未完成事实保留供下次恢复。

## Non-goals

分布式 exactly-once、云队列、跨设备 worker、长期任务历史和 UI progress。

## Ticket-specific stop

如果幂等必须依赖内存 HashMap、sleep 顺序或删除 durable facts，停止并回到 SQLite claim/idempotency 设计。
