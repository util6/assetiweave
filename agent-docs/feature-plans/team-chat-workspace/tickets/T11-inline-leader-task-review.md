# T11：实现 Leader 单 composer 任务模式与内联审核卡

## Outcome

Leader Session 的固定 composer 可以切换到团队任务模式；提交后后台生成草稿，并在 Leader 时间线中以内联计划卡完成任务编辑、推荐分配、排序和一次确认。

## Blocked by

T09。复用已落地 draft/review/confirm AppService workflow。

## Read

- Contracts：TW-P01～TW-P03、TW-U04、TW-D03、TW-B01、TW-B02、TW-B04。
- Seams：S01、S02、S09～S15；Tests TS01、TS05、TS06、TS07。
- Gates：G0、G1、G2、G7。

## Red test first

Leader 活动时进入任务模式并提交：draft 应成为 Leader timeline 中的 plan card；确认前 fake Teammate execution count 为零。用户修改 title/description、owner 和 order 后确认，AppService 持久化审核结果并只执行确认版本。当前 UI 使用独立表单且 review 不覆盖完整可编辑字段。

## Execution steps

1. 在 Leader composer 添加显式 normal/task mode；Teammate Session 不显示任务模式入口。完成标准：进入和退出模式不创建 TeamRun，recipient 始终是 Leader。
2. 通过现有 background workflow 提交 draft，并把 TeamRun/TeamTask snapshot 投影为 timeline plan card。完成标准：drafting/failed/awaiting-review 状态就地更新，不新增 transcript 表。
3. 让审核 workflow 支持 Issue #21 允许编辑的 task title、description、owner 和 order。完成标准：AppService 校验 revision、frozen roster、eligible owner 和非空字段，transport/CLI 同步契约。
4. 构建内联 review card。完成标准：显示 Leader recommendation，用户可编辑/分配/排序，错误定位到具体 task。
5. 连接 confirm gate。完成标准：只有 AppService confirm 成功后显示 executing；双击/重投幂等，确认前 Teammate execution 为零。

## Acceptance

- [ ] 任务模式只在 Leader Session 可用，单 composer 可明确进入/退出。
- [ ] draft 后台运行并以内联 plan card 显示，不阻塞页面或展开独立长表单。
- [ ] 用户可编辑 task title/description、eligible owner 和 order；推荐 owner 默认可见。
- [ ] revision/roster/owner/field 校验由 AppService 执行，所有 transport 一致。
- [ ] confirm 前 Teammate execution count 为零；confirm 成功后 plan 冻结并开始既有调度。
- [ ] plan card 从 Team facts 构造，不写 Team transcript/Conversation。

## Non-goals

改变任务失败/移交规则、自动 quota 排序、Teammate task projection、progressive restore。

## Ticket-specific stop

如果审核字段只能存在前端、需要绕过 revision/confirm gate，或 task mode 需要第二个固定表单，按 Stop protocol 报告。
