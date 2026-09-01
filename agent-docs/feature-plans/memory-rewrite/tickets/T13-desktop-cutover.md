# T13：完成 Desktop、设置与全局任务状态切换

## Outcome

Memory 导航只显示「近期」「回忆」；设置提供生成/使用开关、排除与四类 Agent/model assignment；Memory 后台任务进入全局指示器、close guard 和 event+polling 恢复。

## Blocked by

T05、T07、T10、T11、T12。后端、两条用户 workflow 和公共合同必须稳定。

## Read

- Contracts：C-D06、C-A01、C-A04、C-M04、C-R02、C-S01、C-X01、C-X02。
- Seams：S10、S13–S16；Tests TS06。
- Gates：G0、G3、G4、G7。

## Authority changed

无新持久 Authority；frontend settings/provider/router 投影切换到新合同。

## Red test first

路由/menu 测试只发现 recent/recall 两个子页面；打开应用根 provider 后即使从未访问 Memory，也能通过 polling/event 看到后台 Job；设置更改经 service 持久化，刷新页面后保持；运行中只禁用冲突动作。

## Execution steps

1. 将 Memory router/menu/loader 改为两个子页面。完成标准：旧 Overview/Dream/Library route 不可导航，默认进入 Recent。
2. 将 T07/T10 页面整合为统一 Memory workspace。完成标准：空、错、未配置 Agent、无项目目录、无结果状态独立。
3. 实现设置 UI 与 service。完成标准：开关、排除和四类 assignment 持久化并有 schema test。
4. 重构 MemoryTaskProvider 与全局 indicator。完成标准：应用启动订阅，event 丢失可 polling 收敛，任务状态跨页面可见。
5. 接入 close guard 和响应性。完成标准：运行中退出提示；筛选、导航、设置查看和无关 CRUD 保持可用。
6. 清理用户可见 legacy 文案与组件引用。完成标准：DOM/i18n/router 无 Dream/candidate/Evidence 管理语义。

## Acceptance

- [ ] 只存在「近期」「回忆」两个用户子页面。
- [ ] 默认 Memory 路由进入「近期」。
- [ ] 所有设置通过 service/AppService 持久化。
- [ ] 后台任务无需访问 Memory 页面即可可见。
- [ ] event+polling 能从漏事件恢复。
- [ ] close guard 提示运行中任务。
- [ ] UI 不以页面级 busy 禁用无关操作。
- [ ] 用户可见内容无旧产品术语和内部字段。

## Non-goals

旧 schema 归档/删除、最终全仓验收、通用多 Agent 页面。

## Ticket-specific stop

如果任务 provider 需要页面 mount 才启动，或设置必须保存在组件 localStorage，停止并报告。
