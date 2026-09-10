# T07：实现 Team 多成员 Parallel / Single 工作区

## Outcome

Team 成员以 AionUi 风格 tabs 和独立 chat lanes 呈现：宽屏可并行观察，成员过多可横向滚动，single 可聚焦一个成员，所有 lane 保持独立 Session、滚动和 Composer。

## Blocked by

T04。

## Scope

- Requirements：R-TEAM-001–005、R-NFR-004–007
- Contracts：C-020–C-033
- Seams：S-BE-07、S-FE-02–04、S-FE-07；AionUi TeamPage/Tabs/ViewToggle
- Gates：G-06、G-08、G-10、G-11

## Preflight

1. 确认 T04 已 verified，interactive shared Workspace 的 callback/capability 边界稳定。
2. 在固定 AionUi 提交中读取 TeamPage、TeamTabs、TeamViewToggle、TeamChatView。
3. 定位 roster 排序、active member、每成员 Session/store 与现有 responsive primitives。
4. 固定 1/2/3/6 成员和 320/768/1024/1440 宽度矩阵，记录 baseline screenshots/tests。

## Red tests

1. roster 顺序 Leader first，其余按 sort_order。
2. 两成员/三成员在宽屏各有一个独立 lane。
3. 三成员保持 400px floor 并横向 overflow；两成员可在容器允许时降到 240px。
4. parallel → single 使用触发成员；single → parallel 保留 active member。
5. 每 lane 有正确 recipient composer。
6. inactive lane 更新不抢 active focus、vertical scroll 或 horizontal position。
7. 320/768 temporary single 不覆盖 desktop preference。
8. tabs keyboard Arrow/Home/End 与 aria-selected 正确。

## Implementation steps

1. 对照 AionUi TeamPage、TeamTabs、TeamViewToggle 和 TeamChatView。
2. 从 TeamWorkspaceShell 抽取 Team rail/header/member tabs/member lane composition。
3. 每个 lane 只组合共享 AgentSessionWorkspace，不定义本地 message renderer。
4. viewMode 按 Team ID 保存；响应式强制状态与 preference 分离。
5. 为每 sessionRef 保存 lane-local scroll/follow/new activity。
6. 实现 horizontal overflow/snap/arrows 或等价可访问导航。
7. 更新 loading/restoring/member unavailable 状态，历史内容保持可读。
8. 使用 selector/memo 避免一个 lane delta 重渲染全部 lanes。

## Acceptance criteria

- [ ] AionUi parallel/single 信息架构落地。
- [ ] 每成员 lane 使用同一 shared Workspace。
- [ ] lane recipient、Session ref、status 不串线。
- [ ] inactive member 实时更新但不打断当前操作。
- [ ] 窄屏可用且不丢 desktop preference。
- [ ] Team 页面无大 hero/nested-card 主结构。
- [ ] 主题、键盘、focus 合规。

## Verification

- Team layout/component integration tests；
- render count/selector tests；
- 320/768/1024/1440 screenshots；
- 两/三/多成员真实桌面验收；
- G-08/G-10/G-11。

## Non-goals

- 改变 Team role/roster persistence；
- Board mode；
- Plan/Task workflow 迁移（T08）；
- Memory observer。

## Handoff

完成后 T08 进入 frontier。记录 TeamWorkspaceShell 剩余 domain responsibilities。
