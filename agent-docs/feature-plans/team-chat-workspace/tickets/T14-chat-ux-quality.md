# T14：收敛聊天工作台的滚动、响应式与可访问性

## Outcome

Team 聊天工作台在桌面和窄窗口中保持安静、稠密、可重复操作；滚动不打断阅读，工具活动可折叠，头像、composer、审核和跳转支持键盘与可见焦点。

## Blocked by

T13。

## Read

- Contracts：TW-U01、TW-U03、TW-U05、TW-U06、TW-B06。
- Seams：S13～S15；Tests TS06。
- Gates：G0、G1、G7。

## Red test first

用户滚动到旧消息后注入 delta，scroll position 不被强制到底部；用户接近底部时 delta 自动跟随。键盘可以切换成员、进入 composer、操作 review card 和 jump，窄屏仍显示当前 recipient。

## Execution steps

1. 实现 near-bottom auto-follow、new activity affordance 和恢复时的稳定 anchor。完成标准：组件测试覆盖跟随与不跟随两条分支。
2. 收敛 text/thinking/tool/task/error/restore 的视觉层级；tool detail 可折叠。完成标准：不引入 raw color/Team-only primitive，长内容不破坏 timeline。
3. 完成 member navigation、composer mode、send、review、confirm、task jump 的键盘与 focus 管理。完成标准：操作顺序可预测，活动项有可见 focus。
4. 完成窄窗口 responsive behavior。完成标准：次级导航可折叠，active member 与 recipient 永远可见，composer 可达。
5. 运行真实桌面视觉检查并记录截图/短视频。完成标准：覆盖 idle、streaming、tool、awaiting-review、executing、partial/unavailable 和窄屏。

## Acceptance

- [ ] near-bottom 时自动跟随；阅读旧内容时新 delta 不夺取滚动位置。
- [ ] rich activity 可扫描、tool detail 可折叠、错误与 restore 状态定位清晰。
- [ ] 关键操作可用键盘完成并有可见焦点。
- [ ] 窄屏保持 active member、recipient、timeline 和 composer 可用。
- [ ] 只使用现有 semantic tokens/foundation/common，布局无 form-first 回退。
- [ ] Tauri 桌面证据覆盖规定状态。

## Non-goals

全应用视觉重构、emoji/社交 presence、并排多窗口、动画系统或新设计系统。

## Ticket-specific stop

如果需要改造全局 design system、隐藏当前 recipient，或只能靠固定像素适配单一窗口，按 Stop protocol 报告。
