# T08：建立前端 per-member Session store

## Outcome

前端通过 services 按 TeamMember 维护独立 timeline、活动 task、restore 状态和 transient event projection，并能合并订阅与 polling snapshot。

## Blocked by

T07。

## Read

- Contracts：TW-U02～TW-U05、TW-E02～TW-E06、TW-R03～TW-R04、TW-B03～TW-B05。
- Seams：S12、S13；Tests TS02、TS06、TS07。
- Gates：G0、G1、G5。

## Red test first

controllable event source 同时向 Leader 与 Teammate 发送 delta，再重复一条 event 并返回 polling snapshot；store 必须生成两个隔离 timeline、每个一个 logical response，且 inactive member 状态更新。当前前端只有 TeamRun task 列表和单一 Leader final text。

## Execution steps

1. 扩展 frontend types/schema/services 解析 member turn、Session Event、stream snapshot、replay 与 restoration status。完成标准：所有 desktop 调用和 listen 封装在 services。
2. 实现纯 event reducer，按 member/execution/item/sequence 合并 delta、snapshot、tool 和 terminal。完成标准：可单元测试，重复/乱序/overlap 幂等。
3. 在现有 background task/provider 模式上建立 Team Session store。完成标准：每个 member 独立 state，订阅为主、polling fallback，有界缓存来自 backend snapshot 而非 local persistence。
4. 暴露最小 hooks/selectors。完成标准：页面只读取活动 member projection 和 avatar status，不理解 transport/Vendor。
5. 处理 Team 切换与 provider cleanup。完成标准：旧 Team listener 被解除，活动后台 task 仍由全局 provider 跟踪。

## Acceptance

- [ ] Leader/Teammate events 按 member 严格隔离，切换 selector 不移动 items。
- [ ] duplicate、out-of-order、delta/snapshot、tool result 和 replay/live overlap 均正确合并。
- [ ] subscription miss 后 polling snapshot 收敛到同一 logical timeline。
- [ ] inactive member 的 running/unread/completed/failed 状态可独立读取。
- [ ] frontend 代码不直接 invoke/listen，不持久化 transcript 到 localStorage/settings/mock。

## Non-goals

chat layout、composer、message rendering、task review 和 task projection UI。

## Ticket-specific stop

如果 store 必须把所有成员合成一个 message array、需要 Vendor 分支，或需要 localStorage 保持正文，按 Stop protocol 报告。
