# T12：接入全局进度、轮询与退出保护

## Outcome

Team draft/restore/execute 在后台运行时，Team 页面和全局区域持续显示进度；页面切换或漏掉事件后由轮询恢复；应用退出前提示仍在运行的 Team 工作。

## Blocked by

T11。

## Read

- Contracts：C-A02–C-A04、C-S01。
- Seams：S07、S09、S10、S12–S14；Tests TS04、TS05、TS06。
- Gates：G0–G5。

## Red test first

Frontend provider 收到一次 running event 后卸载/重挂并漏掉 terminal event；通过 list/get polling 必须恢复终态。活动 TeamRun 存在时 close prompt 显示，终态后不再提示。

## Execution steps

1. 把 Team background task projection 接入现有 TaskRuntime/BackgroundTaskRegistry。完成标准：draft/restore/execute 快速返回 snapshot，cancel 进入共享生命周期。
2. 建立 frontend TeamTaskProvider 的 event+poll reconciliation。完成标准：重复 snapshot 幂等，漏事件和页面重挂可恢复。
3. 增加 Team-local 与 global indicator。完成标准：离开 Team 页后仍能发现活动工作，返回后状态一致。
4. 收窄 disabled state。完成标准：只禁用冲突 Team mutation，过滤、查看、设置和无关 CRUD 可用。
5. 接入 App close report。完成标准：活动 Team 工作触发提示，terminal/无任务不提示，关闭等待有界。

## Acceptance

- [ ] start 命令快速返回，不 await 完整 Agent 工作。
- [ ] event 和 polling 得到同一 task projection。
- [ ] 页面切换、provider 重挂和漏事件后状态恢复。
- [ ] Team-local/global progress 对同一任务一致且无重复。
- [ ] 无关 UI 操作在 Team 执行时仍可用。
- [ ] 活动工作触发退出提示；结束后提示消失。

## Non-goals

通知中心重构、跨应用系统通知、后台云执行和新的通用 TaskRuntime。

## Ticket-specific stop

如果页面需要 page-level busy 覆盖整个 Team workspace，或另建第二个任务 registry，停止并复用现有 provider/runtime 模式。

