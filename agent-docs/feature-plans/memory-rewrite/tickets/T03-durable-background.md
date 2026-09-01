# T03：完成 durable Memory 后台恢复管线

## Outcome

应用无需进入 Memory 页面即可调度 Phase 1；queued/running/retry Job 在启动、lease 过期、heartbeat 丢失、取消和进程重启后得到确定结果，并通过 TaskRuntime 对外投影进度。

## Blocked by

T02。需要 durable Job 与最小 Phase 1 worker。

## Read

- Contracts：C-A01、C-A04、C-P01、C-P02、C-P04–C-P07、C-P09、C-S02、C-S03。
- Seams：S03、S04、S05、S06、S10、S14；Tests TS02。
- Gates：G0、G1、G2、G7。

## Authority changed

扩展 Job 生命周期为可恢复 authority；TaskRuntime 继续只保存活动投影。

## Red test first

让 worker 取得 lease 后模拟进程中断，推进时钟超过 lease 并重建 AppRuntime：Job 被新 ownership token 领取并完成。失败 Job 到 `retry_at` 前不运行，到期后自动运行；取消终态不会被恢复扫描重新执行。

## Execution steps

1. 完整实现 Job state、ownership token、lease、heartbeat、retry count/at、watermark、last error 转换。完成标准：所有转换由 repository 原子校验 token。
2. 接入启动漏单扫描、consumer cutoff/backfill、过期 lease 恢复和到期 retry scheduler。完成标准：不依赖页面或新 Conversation 才继续。
3. 复用 TaskRuntime 注册 Memory task、dedup/conflict、progress、cancel 与 retention。完成标准：Task snapshot 可由持久 Job 重建，TaskRuntime 清空不丢工作。
4. 接入 shutdown/close guard。完成标准：运行中任务可见，worker 停止领取新 Job并持久化可恢复状态。
5. 添加有界并发与水位合并。完成标准：同 Session 单 worker；重复更新只形成一个有效当前/后继工作。

## Acceptance

- [ ] 启动自动 backfill、漏单扫描和 retry。
- [ ] lease 过期可安全接管，旧 token 无法提交结果。
- [ ] heartbeat 防止活 worker 被误接管。
- [ ] 失败按退避到期自动重试。
- [ ] 取消、shutdown 和 restart 状态明确且幂等。
- [ ] TaskRuntime 清空/重建不改变 SQLite Job 事实。
- [ ] 阻塞 I/O 和 Agent 调用期间不持有全局 app lock。

## Non-goals

Project/Global consolidation、Recent 页面、Recall、完整 CLI。

## Ticket-specific stop

如果恢复需要把 prompt/正文写入 Task snapshot，或 Job state 只能驻留内存，停止并报告。
