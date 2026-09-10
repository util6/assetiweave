# T08：迁移 Team Plan、Task 与成员运行操作

## Outcome

Leader task mode、计划审核、TeamTask、成员定位、停止/中断/队列/恢复在新 parallel/single 工作区中保持完整；共享聊天组件仍不拥有 Team 业务 Authority。

## Blocked by

T07。

## Scope

- Requirements：R-TEAM-006、A-003/A-007
- Contracts：C-030–C-033、C-092
- Seams：S-BE-07、S-FE-02–05
- Gates：G-06、G-08、G-11

## Preflight

1. 确认 T07 已 verified，并读取 parallel/single 的 state ownership 与 handoff。
2. 逐一定位 Plan review、TeamTask、normal/task mode、stop/interrupt/queue/restore 的现有 Authority 和 tests。
3. 记录 Team workflow targeted tests 与持久化事实 baseline。
4. 建立功能迁移清单；每个旧入口只有在新入口已有 Green 证据后才能删除。

## Red tests

1. Leader lane 可切 normal/task mode；Teammate 无 task mode。
2. draft/review/confirm 仍调用现有 service 并遵守 revision。
3. Plan card 位于 Leader chronology，awaiting_review 可编辑。
4. TeamTask 只显示在 owner lane。
5. 从 Plan Task 导航：parallel 横向定位；single 切 active member；anchor/focus 正确。
6. busy member stop/interrupt/queue 只影响该成员。
7. restore/runtime failed/session stopped 有对应操作且不覆盖历史。
8. TeamRun/TeamTask SQLite facts 与 UI 操作一致。

## Implementation steps

1. 保留 TeamPage/TeamWorkspace 的 domain orchestration，把 typed Plan/Task slots 注入 shared timeline/lane。
2. 将现有 composer normal/task mode 适配到 Team capability callbacks。
3. 连接 stop/cancel/interrupt/retry/queue 的已有 AppService/service。
4. 将 task navigation 与 member tabs/lane anchors 集成。
5. 处理 workflow error、revision conflict、partial restore 与 member unavailable。
6. 删除新 UI 中重复的 domain state，但保留旧 compatibility API 到 T11。
7. 回归 #19/#21 的 authority 与安全测试。

## Acceptance criteria

- [ ] 计划到执行完整 Team flow 可用。
- [ ] TeamTask ownership/ordering/revision 不变。
- [ ] 操作只影响目标成员且通过 service/AppService。
- [ ] shared AgentSessionWorkspace 不 import Team service/model。
- [ ] Plan/Task 与聊天 chronology/定位正确。
- [ ] 失败/恢复不清空历史。

## Verification

- Team workflow services/tests；
- Team page component tests；
- targeted Rust team application tests；
- 真实 draft → review → confirm → member execution 演示；
- G-08/G-11。

## Non-goals

- 修改 Team 调度/ownership/fallback；
- 添加 Board mode；
- Memory integration；
- Legacy contract 删除。

## Handoff

Team 迁移完成。T11 仍等待 T10；列出可在 T11 删除的旧 Team-only APIs/components。
