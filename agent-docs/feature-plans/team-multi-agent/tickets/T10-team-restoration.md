# T10：恢复完整 Team 与部分失效成员

## Outcome

用户重新打开 Team 时先恢复 Leader 主聊天，再后台恢复各 Teammate；健康成员可继续，失效成员明确显示 unavailable，原任务 owner 不变。

## Blocked by

T03、T09。

## Read

- Contracts：C-D03、C-R03–C-R05、C-H01–C-H05、C-P05、C-A02、C-S01。
- Seams：S01–S07、S09、S12–S14；Tests TS01、TS02、TS03、TS05。
- Gates：G0–G4。

## Red test first

fake runtime 配置 Leader 和 Teammate A 可恢复、Teammate B dead anchor。打开 Team 后 Leader 时间线恢复，A=ready，B=resume-unavailable；B 的既有任务仍归 B，且任何 replay fixture 都未产生新 mailbox/tool/task mutation。

## Execution steps

1. 建立 Team restore workflow 和 per-member runtime projection。完成标准：SQLite Team facts 与 Provider context 结果分别加载，不互相覆盖。
2. 实现 Leader-first、Teammate-background 恢复。完成标准：Leader timeline 可先呈现；各成员独立转为 ready/unavailable。
3. 增加 replay suppression/epoch guard。完成标准：恢复历史只进入允许的临时投影，工具和持久 mutation 次数不变。
4. 接入成员状态、blocked task 展示和显式重新初始化入口边界。完成标准：本卡不静默新建；需要新上下文时必须是后续用户明确操作。

## Acceptance

- [ ] Leader history 恢复不等待所有 Teammate 完成。
- [ ] 健康成员不受单个 dead anchor 影响。
- [ ] unavailable 成员不获得伪造的新 Session。
- [ ] unavailable 成员的任务仍归原 owner，不移交。
- [ ] Teammate replay 不进入 Leader timeline。
- [ ] 所有 replay 不产生第二次工具/mailbox/task 副作用。

## Non-goals

应用进程崩溃后的 coordinator recovery、自动修复 dead Session、跨设备同步和 Provider transcript 备份。

## Ticket-specific stop

如果 UI 必须读取 Conversation 数据才能显示 Leader 历史，或 restore 需要修改 TeamRun 事实状态来表达每个成员进度，停止并修正投影边界。

