# T12：投影 TeamTask 到成员时间线并提供 Leader 跳转

## Outcome

确认后的 TeamTask 出现在 owner Teammate 的时间线，状态随执行更新；Leader plan card 聚合全局状态，点击子任务切换到 owner 并定位对应锚点。

## Blocked by

T10、T11。

## Read

- Contracts：TW-D03、TW-P03～TW-P05、TW-U02～TW-U05、TW-E03。
- Seams：S01、S02、S08、S12～S15；Tests TS01、TS06、TS07。
- Gates：G0、G1、G2、G7。

## Red test first

确认两个 task 分别归属不同 Teammate 后，每个 timeline 只能出现自己的 task card；Leader card 点击 task A 切换到 owner A 并滚动到稳定 anchor。重新加载 Team facts 后卡片仍存在，且没有 transcript 记录。

## Execution steps

1. 定义 TeamTask → timeline structured item projection 和稳定 anchor。完成标准：identity 来自 durable task ID，和 Provider message identity 不冲突。
2. 将 queued/running/succeeded/failed/canceled snapshot 更新合并到 owner timeline。完成标准：状态原位更新，task result 与 Provider正文 Authority 清晰分开。
3. 在 Leader plan card 聚合 child states、owner 和进度。完成标准：聚合来自 TeamRun snapshot/event，不扫描成员正文。
4. 实现 plan task → member + anchor navigation。完成标准：切换活动头像后定位 task；导航不触发 review、reassign 或新 Session。
5. 加入 restart reconstruction 测试。完成标准：只给 TeamRun/TeamTask facts 即可重建卡片，Conversation/Team transcript 行数不变。

## Acceptance

- [ ] 每个 confirmed TeamTask 只投影到其 owner timeline。
- [ ] task 状态原位更新，失败/取消仍保留原 owner。
- [ ] Leader plan card 正确聚合所有 child state 与 owner。
- [ ] 点击任务切换到正确成员并定位稳定 task anchor，不改变业务事实。
- [ ] task cards 可从 SQLite facts 重建，不依赖聊天正文或 local cache。
- [ ] direct Teammate chat 不创建、转移或完成 TeamTask。

## Non-goals

自动重新分配、跨成员 handoff、从 Agent 正文推断任务状态、历史 replay 调度。

## Ticket-specific stop

如果必须把 task card 保存为 chat message、从正文解析 task 状态，或 jump 会修改 owner，按 Stop protocol 报告。
