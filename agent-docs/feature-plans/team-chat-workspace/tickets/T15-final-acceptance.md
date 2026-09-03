# T15：完成 Issue #21 最终验收与独立 Review

## Outcome

Issue #21 的 P0 行为、Authority、Provider adapters、transport、chat UX、恢复、隐私和回归矩阵具有可复现证据；旧 form-first 主路径不再被产品使用。

## Blocked by

T14 及所有前置 Checkpoint findings 已关闭。

## Read

- Contracts：TW-D01～TW-B06 全部。
- Seams：S01～S18；Tests TS01～TS08。
- Gates：G0～G8。

## Red test first

建立 Issue #21 acceptance matrix，先运行所有目标与全量门禁；任何缺失行为、旧路径仍被 UI 调用、generated contract 漂移或隐私 marker 泄漏均视为 Red，不以“主要功能可用”跳过。

## Execution steps

1. 将 Issue #21 User Stories/P0 决策映射到自动测试或桌面证据。完成标准：每项有唯一证据位置，无“代码阅读”占位。
2. 运行 G0～G8 并修复本阶段引入的回归。完成标准：所有可运行 gate 绿色；环境阻塞记录精确命令、错误和未验证风险。
3. 清理旧产品路径。完成标准：Team UI 不再调用 blocking final-text Leader chat 或展示独立长 workflow 表单；兼容 backend/CLI 若保留，有测试和明确调用者。
4. 执行 Authority/隐私审计。完成标准：Provider正文、Team facts、Agent binding、transient projection 各自唯一；Conversation/log/snapshot 无正文/anchor。
5. 使用新上下文执行 Checkpoint Review prompt。完成标准：P0/P1 findings 全部修复并有回归测试；P2 明确记录。
6. 完成桌面 smoke 和视觉证据。完成标准：ACP fixture/可用 smoke 与 Antigravity fixture/可用 smoke 均覆盖 create/resume/replay/live；真实 Provider 不可用时不伪报。

## Acceptance

- [ ] Issue #21 每项 P0 行为均有可复现自动化或桌面证据。
- [ ] G0～G8 全部通过，或外部环境阻塞被精确记录且不掩盖代码失败。
- [ ] Team UI 是 chat-first 单时间线体验，支持成员切换、直聊、streaming、inline plan、task projection 和 progressive restore。
- [ ] ACP 与 Antigravity 都通过语义 Session Adapter；Team/frontend 无 Vendor 分支。
- [ ] Antigravity 使用真实 ID resume、Provider transcript replay 和每轮 CLI process。
- [ ] Team/Conversation/log/durable snapshots 无聊天正文、tool payload、credential 或 Resume Anchor。
- [ ] Engine contract、surface matrix 和 Go CLI 与 AppService 行为一致。
- [ ] 独立 Review 无未解决 P0/P1 finding。

## Non-goals

Issue #21 Out of Scope 全部内容，以及后续产品增强。

## Ticket-specific stop

如果任何 gate 被跳过、Review 与实现使用同一未经重置上下文、或旧路径只能靠保留第二套 UI 才能通过，按 Stop protocol 报告。
