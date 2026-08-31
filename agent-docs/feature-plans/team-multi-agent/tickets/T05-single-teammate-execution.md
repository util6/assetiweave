# T05：确认并执行单个 Teammate 任务

## Outcome

用户确认一个任务的审核分配后，系统只调用选中的 Teammate，持续显示状态和结果；失败或取消仍保留原 owner。

## Blocked by

T04。

## Read

- Contracts：C-P03–C-P05、C-R01、C-R02、C-A01、C-A02、C-A04、C-S01。
- Seams：S01、S02、S07、S09、S10、S12–S14；Tests TS01、TS04、TS05。
- Gates：G0–G4。

## Red test first

把 Leader 推荐 owner 改为另一个 Teammate 后确认：fake runtime 必须只收到用户确认 owner 的 Persistent request；成功、失败和取消都不调用其他成员。

## Execution steps

1. 建立原子 confirm transition 和不可变 reviewed assignment。完成标准：重复确认幂等，非法 owner/过期 revision 被拒绝。
2. 调度一个 TeamTask 到选定成员 context。完成标准：状态从 queued/running 到 terminal，结果作为 TeamTask 结构化结果可读。
3. 接入 task snapshot、取消、Engine/CLI 和 UI。完成标准：用户能看到 owner、阶段、结果或安全错误，只有冲突操作被禁用。
4. 补成功、失败、取消和 disconnect 行为。完成标准：任务 owner 始终不变，其他 member runtime 未被调用。

## Acceptance

- [ ] confirmation 原子冻结用户审核映射并开始执行。
- [ ] 推荐映射与用户映射不同时，以用户映射为唯一执行输入。
- [ ] 任务成功、失败、取消状态在重读后稳定。
- [ ] failure/cancel/disconnect 不触发 fallback、移交或自动 retry。
- [ ] Persistent Teammate context 保留供后续使用。

## Non-goals

Leader 最终总结、TeamMailboxMessage、Team MCP、CLI Agent 工具、多任务并行和应用重启恢复。

## Ticket-specific stop

如果 dispatch 只能由前端 effect 驱动，或需要把 TaskRuntime 当作 TeamTask 权威存储，停止并回到 AppService+SQLite。
