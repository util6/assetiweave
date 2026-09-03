# T06：建立后台 member turn、replay 与 stream snapshot workflow

## Outcome

AppService 可以为任意 TeamMember 启动后台直聊 turn 或 history replay，快速返回 task/stream snapshot，并通过 transient projection 提供活动事件和取消。

## Blocked by

T03、T04、T05。

## Read

- Contracts：TW-D01～TW-D04、TW-E02、TW-E04、TW-E06、TW-R03～TW-R06、TW-B01、TW-B02、TW-B05。
- Seams：S01～S03、S08、S18；Tests TS01、TS02、TS07、TS08。
- Gates：G0、G2、G5。

## Red test first

通过 AppService 向 Teammate 发送消息：调用应在 fake Provider 阻塞前返回活动 task/stream snapshot，随后可按 member/execution 读取增量事件；旧 `team_leader_chat` 会阻塞并只返回 final text。

## Execution steps

1. 新增 member-scoped turn/replay request，校验 tenant、Team、member、semantic capability 和稳定 context key。完成标准：Leader/Teammate 共用一条 workflow，非法跨 Team member 被拒绝。
2. 复用 TaskRuntime 启动后台 execution，绑定通用 event sink 与 transient projection。完成标准：公开 start 快速返回，不持有全局 app lock。
3. 提供按 task/stream/member 读取 snapshot 和活动状态的 AppService 查询。完成标准：正文只来自 transient projection，durable task snapshot 只含安全元数据。
4. 支持取消与终态收敛。完成标准：取消只影响目标 turn/replay，不终止其他成员或改变已确认 task owner。
5. 保留现有 Leader blocking API 作为兼容入口，但新 workflow 不调用它。完成标准：现有 CLI/tests 不回归，后续 UI 有独立 streaming 路径。

## Acceptance

- [ ] Leader 与 Teammate 使用同一 member-scoped AppService workflow 和各自 context key。
- [ ] start 在 Provider 完成前返回 task/stream snapshot，增量事件随后可读。
- [ ] 页面/consumer 缺席不取消后台工作；重新读取 snapshot 得到当前投影。
- [ ] cancel、failure、timeout 只影响目标执行并产生结构化终态。
- [ ] Team/Conversation/operation log/durable task snapshot 不保存 prompt 或 event body。
- [ ] member 不存在、跨 Team、缺 capability 和 invalid anchor 返回稳定错误。

## Non-goals

Tauri/Engine/CLI exposure、frontend services、chat UI、任务审核 UI。

## Ticket-specific stop

如果必须让 TaskRuntime 成为持久 transcript、让 frontend 传 Provider anchor，或复制 Leader/Teammate workflow，按 Stop protocol 报告。
