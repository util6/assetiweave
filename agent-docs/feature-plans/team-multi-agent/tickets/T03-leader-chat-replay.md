# T03：接入 Leader 主聊天与历史回放

## Outcome

用户可以向 Team Leader 发送消息；离开并重新打开 Team 后，从 Leader Provider Session 恢复旧用户/Leader 正文并继续对话，AssetIWeave 不保存重复 transcript。

## Blocked by

T01、T02。

## Read

- Contracts：C-D03、C-D04、C-L02、C-R03–C-R05、C-H01–C-H05、C-A01、C-S01。
- Seams：S01–S04、S06、S09、S10、S12、S14、S15；Tests TS01、TS02、TS03、TS05。
- Gates：G0–G4。

## Red test first

AppService 使用带历史 fixture 的 fake runtime 向 Leader 执行一次消息，释放并重开 Team 后必须返回 Provider replay 的时间线；Team 和 Conversation 表中不存在消息正文。

## Execution steps

1. 为 Leader message/restore 建立 AppService workflow 和临时时间线 DTO。完成标准：持久层只保存 context key/状态，不保存正文。
2. 在 ACP runtime 区分 replay event 与 live event。完成标准：恢复历史可读取，但 replay 不被当作新输出或 mutation。
3. 接入 Tauri/Engine、frontend service 和 Leader workspace。完成标准：发送、loading、错误、重开恢复均从 service 驱动。
4. 在成员选择处执行 Leader Resume+Replay 能力门。完成标准：只有 Resume 的 Agent 仍可作为 Teammate 候选，但不进入 Leader 候选。

## Acceptance

- [ ] 新消息进入 Leader Persistent context，刷新/重开后可继续。
- [ ] 旧正文来自 Provider replay/read，不来自 Team/Conversation 表。
- [ ] replay 与 live event 可区分，恢复不会重复显示同一 live entry。
- [ ] Team 浏览不会触发 Conversation 同步、Memory 或搜索写入。
- [ ] 不满足 Resume+Replay 的 Agent 不能保存为 Leader。

## Non-goals

TeamRun、任务草稿、Teammate 执行、mailbox、MCP、完整 Team restore 状态。

## Ticket-specific stop

Provider 只能恢复模型上下文但不能读取历史时，不解析 Conversation Adapter 记录补齐正文；报告 capability 缺口。

