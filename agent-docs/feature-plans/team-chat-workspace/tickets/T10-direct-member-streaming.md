# T10：实现当前成员直聊与 rich live rendering

## Outcome

固定 composer 可以向当前 Leader 或 Teammate 发送普通消息；用户消息立即出现，Agent text、processing/thinking、tool、terminal/error 流式原位更新，切换成员不终止工作。

## Blocked by

T09、T06。

## Read

- Contracts：TW-U02～TW-U06、TW-E01～TW-E06、TW-A02、TW-B02、TW-B04～TW-B06。
- Seams：S01、S03、S08、S12～S15；Tests TS01、TS02、TS06、TS07。
- Gates：G0、G1、G2、G7。

## Red test first

在 Teammate 活动时发送消息，立即看到带正确 recipient 的 user item；fake Provider 发出 text delta、processing、tool start/result 后同一 timeline 原位更新。切到 Leader 再切回，Teammate execution 仍继续且无重复。

## Execution steps

1. 将 composer normal mode 连接 member turn start service。完成标准：发送使用 active member ID，成功 start 前后有明确 optimistic/accepted/error 状态。
2. 构建通用 message/activity renderer，覆盖 text、processing/thinking、tool lifecycle、notice、terminal 和 error。完成标准：renderer 只消费 generic items，不检查 Agent ID/protocol。
3. 实现 stable in-place updates 与 inactive status。完成标准：delta 不增加新 assistant 卡，tool result 归属原 tool，切换不取消 execution。
4. 处理 Provider fidelity。完成标准：缺 thought/tool 时显示声明的 processing/限制，不合成正文。
5. 锁定 composer recipient、busy 和冲突动作。完成标准：只禁用目标 member 的冲突发送；导航和其他成员阅读可用。

## Acceptance

- [ ] Leader 与 Teammate 均可通过同一 composer normal mode 直聊。
- [ ] user item 立即出现，backend rejection 就地标记，不把失败消息发给其他 member。
- [ ] text/thinking/tool/terminal/error 按 generic event 原位更新且无重复。
- [ ] 切换成员、离开页面再返回不取消活动 turn，inactive status 正确。
- [ ] 不支持的 Provider rich events 被明确表达，不伪造隐藏推理。
- [ ] Team/Conversation/local settings 不持久化正文。

## Non-goals

Leader Team task mode、任务审核、task projection、应用重启 replay。

## Ticket-specific stop

如果 composer 需要按 Agent 写发送分支、message renderer 直接解析 ACP/agy payload，或页面 busy 锁死全应用，按 Stop protocol 报告。
