# T01：抽取共享 Agent Session 兼容工作区

## Outcome

现有 Team 单成员聊天通过领域中立的共享 `AgentSessionWorkspace` 渲染，用户行为保持不变；后续 Team、Memory 和 interactive chat 可在同一组件接缝上扩展。

## Blocked by

None（初始 frontier）。

## Scope

- Requirements：A-001、A-007、A-009、R-NFR-001、R-NFR-005
- Contracts：C-030、C-033
- Seams：S-FE-01、S-FE-02、S-FE-03、S-FE-05、S-FE-07
- Gates：G-00、G-04、G-11

## Preflight

1. 读取 `00-execution-router.md`、本卡及其中指定的产品、接缝与 UI 章节。
2. 记录 `git status --short`，确认没有覆盖其他任务的未提交文件。
3. 定位 `TeamWorkspaceShell`、`TeamSessionProvider/Store` 及现有 Team 行为测试，记录 baseline 结果。
4. 仅建立共享兼容 surface，不提前更改后端合同或视觉体系。

## Current facts

- `TeamWorkspaceShell` 同时包含 timeline、item renderer、scroll 与 composer。
- `TeamSessionProvider/Store` 是现有 Team Session state boundary。
- 当前 Team behavior tests 是本卡回归 Authority。

## Red tests

1. 新共享 Workspace 能以不含 Team 类型的 view model 渲染现有 assistant/processing/error item。
2. interactive capability=true 时显示 composer，并把发送回调交给调用者。
3. observer capability=false 时不显示 composer。
4. Team adapter 渲染结果保留当前可见文案和发送行为。

Red 证据必须来自共享 surface 尚不存在或 Team 尚未通过它渲染，而不是 snapshot fixture 无效。

## Implementation steps

1. 定义最小共享 view/capability props，只覆盖当前 Team 已有行为；不要提前实现 T02/T03 contract。
2. 抽取 Workspace shell、基础 timeline 容器和 composer presentation；数据获取留在外层。
3. 新增 Team projection → shared props 的纯 adapter。
4. 让 Team active member path 使用共享 Workspace；Team plan/task 和导航暂由原外层组合。
5. 保持旧 Team types/service/store 不变，形成 expand 状态。
6. 把 shared component 放在领域中立目录，确保 import graph 不反向依赖 Team/Memory。
7. 使用 semantic Theme Tokens，保留现有视觉以避免本卡混入 AionUi 重做。

## Acceptance criteria

- [ ] Team 当前单成员可查看、滚动和发送，行为测试无回归。
- [ ] 共享 Workspace 源码不 import Team、Memory、Task Center 或 Tauri API。
- [ ] interactive/observer composer 显隐由 capability 驱动。
- [ ] 页面/组件没有新增 direct `invoke(...)`。
- [ ] 旧 Team path 仍可回滚，T11 前无破坏性删除。

## Verification

- targeted shared component tests；
- `TeamWorkspaceShell` / `TeamPage` tests；
- `pnpm typecheck`；
- 按现有 Vitest 参数运行 Team/shared targeted suites；
- 浏览 Team 一成员页面，确认 console 无新增错误。

## Non-goals

- Tool input/output；
- 完整 Turn/Thinking；
- AionUi 视觉重做；
- parallel lanes；
- Memory observer；
- 删除旧 Team DTO/store。

## Handoff

完成后 T02 进入 frontier。记录共享 props、Team adapter 和仍待迁移的 Team responsibilities。
