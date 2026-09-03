# T09：将 TeamPage 重构为 chat workspace shell

## Outcome

用户进入 Team 后看到 GoLutra 风格单聊天工作台：Team 导航、活动成员时间线、成员头像栏和固定 composer；Leader 默认选中，管理表单退居 dialog/secondary action。

## Blocked by

T08。

## Read

- Contracts：TW-U01～TW-U06、TW-D03、TW-B04、TW-B06。
- Seams：S12～S15；Tests TS06。
- Gates：G0、G1、G7。

## Red test first

render Team page 后应默认高亮 Leader/群主、显示其 timeline 与 composer recipient；点击 Teammate 头像后只切换活动 timeline/recipient，TeamTask owner 和后台状态不变。当前页面没有 member Session shell。

## Execution steps

1. 原地拆分当前 TeamPage，使页面 state orchestration 与 chat shell/presentational components 分离。完成标准：沿用现有 Team route 和 TeamPage 入口，主体验只有一条实现路径。
2. 建立 Team navigation、header、single timeline viewport、member avatar navigation 和 fixed composer shell。完成标准：关键区域在正常窗口高度内可达，不依赖滚动到表单底部。
3. 默认选择 Leader，连接 T08 selectors，并让头像显示 role、Agent/model、running/restore/unread status。完成标准：切换只改变 active member view state。
4. 保留 create/edit/delete roster 能力为次级 dialog/action。完成标准：既有 CRUD tests 继续通过，日常聊天页面不展开长表单。
5. 添加空、loading、no-eligible-member 和 error shell。完成标准：都使用 foundation/common 与 semantic tokens。

## Acceptance

- [ ] 页面包含 Team 导航、单 timeline、成员栏和固定 composer 的 chat-first 层级。
- [ ] 首次进入默认 Leader，群主标识和当前 recipient 清晰。
- [ ] 点击头像切换独立 Session，不取消后台工作、不改变 TeamTask owner。
- [ ] inactive member 状态可见，页面不显示并排完整 timelines。
- [ ] Team CRUD 仍可用但不再占据主工作区。
- [ ] 没有 raw palette、frontend direct invoke 或平行 v2/legacy 页面。

## Non-goals

发送消息、rich event renderer、Leader task mode、task projection、progressive replay。

## Ticket-specific stop

如果必须重写全局导航/设计系统、合并成员正文，或删除既有 Team CRUD 才能完成布局，按 Stop protocol 报告。
