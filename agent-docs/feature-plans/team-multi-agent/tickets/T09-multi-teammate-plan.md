# T09：执行多任务、多 Teammate 固定计划

## Outcome

一个确认计划可包含多个 TeamTask 和多个 Teammate；系统严格按用户确认 owner 执行，展示稳定顺序和独立状态，任何失败都不会移交给其他成员。

## Blocked by

T07、T08。

## Read

- Contracts：C-L01–C-L03、C-P03–C-P05、C-R01、C-A02、C-T01、C-S01。
- Seams：S01、S02、S07、S09–S14；Tests TS01、TS04、TS05。
- Gates：G0–G4。

## Red test first

确认三个任务给 A、B、A；让 A 的第一个任务失败。最终调用集合必须仍是 A、B、A，B 不接管失败任务，三个任务 owner/顺序和 terminal 状态与确认计划一致。

## Execution steps

1. 扩展确认和 dispatch 处理多个有序任务。完成标准：输入去重、共享 roster 一次加载、owner 不可变。
2. 通过每个成员独立 context 执行其任务。完成标准：同成员复用 context，不同调用使用不同 execution ID；并发实现不影响确定性状态。
3. 聚合 task board、mailbox 和 Leader summary。完成标准：每个结果只消费一次，UI 不把 Teammate timeline 拼入主聊天。
4. 补失败、取消、disconnect 和 unavailable。完成标准：没有自动 fallback/retry/reassignment，其他任务按自身状态继续或明确取消。

## Acceptance

- [ ] 多任务确认结果逐项成为唯一 owner Authority。
- [ ] 同一 Teammate 的多个任务复用其稳定 context。
- [ ] Teammate 顺序只影响推荐/展示，不触发故障转移。
- [ ] 任一任务失败不改变其他任务 owner。
- [ ] roster 在 awaiting-review/executing 期间不可修改。
- [ ] task board 和 Leader summary 无重复结果。

## Non-goals

通用 DAG、动态 work stealing、quota scheduler、自动 retry/replan 和跨 Provider Session 迁移。

## Ticket-specific stop

如果实现开始推断“更合适成员”或需要在失败时重写 owner，删除该路径并回到确认计划。

