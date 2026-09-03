# T13：实现完整现场的渐进恢复

## Outcome

重开 Team 时立即显示结构化 shell/task facts，优先 replay 活动成员，后台有界恢复其他成员，并正确合并 replay/live，呈现 ready/restoring/partial/unavailable。

## Blocked by

T05、T10、T12。

## Read

- Contracts：TW-D01～TW-D04、TW-E04～TW-E06、TW-R01～TW-R06、TW-A01～TW-A07、TW-B02、TW-B05。
- Seams：S01～S03、S08～S18；Tests TS01、TS02、TS04、TS06～TS08。
- Gates：G0、G1、G2、G5、G7。

## Red test first

重开包含 Leader、两个 Teammate 和已确认 tasks 的 Team：shell/task cards 必须在 fake history unblock 前出现；Leader replay 先启动，inactive member 并发不超过上限。切换到 Teammate 会提升其优先级；replay 期间注入 live event 后最终 timeline 无乱序/重复。

## Execution steps

1. 扩展 restoration snapshot/status，先从 SQLite 返回 Team facts 和每个 member 的恢复状态。完成标准：不等待 Provider I/O 即可渲染 shell/task cards。
2. 建立有界 replay 调度：active member 优先，inactive background prewarm，切换时 reprioritize。完成标准：并发上限可测试，取消旧优先级不丢已收到 items。
3. 实现 replay/live merge boundary。完成标准：live 在 replay 中到达时被正确排序/缓冲，replay completion 不覆盖新 item。
4. 处理 ready、partial、unavailable 和 invalid anchor。完成标准：partial member 可在 Resume 健康时继续直聊；invalid anchor 不静默新建。
5. 验证故障隔离和退出。完成标准：一个 member 失败不阻塞其他 member；活动 replay/turn 出现在关闭报告。
6. 锁定零正文持久化。完成标准：重启前后的 Team/Conversation/log/durable snapshot 均无测试正文 marker。

## Acceptance

- [ ] structured shell、TeamRun/TeamTask 在 history 完成前可见。
- [ ] active member replay 优先，inactive 有界后台恢复，切换可调整优先级。
- [ ] replay/live overlap 无重复、覆盖或逆序。
- [ ] member 显示 not-started/restoring/ready/partial/unavailable；partial 可在 Resume 健康时继续。
- [ ] invalid anchor 明确 unavailable，不静默创建新 Session。
- [ ] 单成员失败不阻塞健康成员，也不清除结构化 Team facts。
- [ ] 恢复流程保持 Conversation 零写入和日志/快照无正文。

## Non-goals

瞬时加载全部历史、Provider 不暴露的 thought/tool 重建、跨设备历史同步、自动 reset Session。

## Ticket-specific stop

如果完整现场只能通过持久化 UI message、所有成员必须串行阻塞恢复，或 live/replay 无法稳定区分，按 Stop protocol 报告。
