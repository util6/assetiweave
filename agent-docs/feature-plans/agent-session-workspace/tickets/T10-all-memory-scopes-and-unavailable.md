# T10：接入 Project、Global、Recall 与 Unavailable 生命周期

## Outcome

Project、Global 和 Recall Memory 使用与 Session Memory 相同的只读执行现场；跨租户、恢复、淘汰和进程重启后的引用有明确 unavailable 行为。

## Blocked by

T09。

## Scope

- Requirements：R-MEM-001–006、R-NFR-003/008
- Contracts：C-010–C-114
- Seams：S-BE-05/06/08/09、S-FE-05/06、S-TEST-03/05
- Gates：G-02、G-03、G-04、G-09、G-11

## Preflight

1. 确认 T09 已 verified，并读取统一 Memory wrapper、Task ref 与 observer handoff。
2. 读取 Issue #20 router，重新定位 Project/Global/Recall 当前 execute/retry/resume 路径。
3. 固定 tenant isolation、eviction、restart、new-attempt 与 publish-failure fixtures。
4. 记录四 scope targeted tests baseline；禁止更改 Memory recipe/evidence 产品语义。

## Red tests

1. Project Memory Agent Stage ref + observer content。
2. Global Memory tenant scope，其他 tenant ref 不可读。
3. Recall persistent 多 Turn/replay/live 不重复，reader tools 可见且 DB path/credential redacted。
4. ref evicted 返回 unavailable，Task View 仍显示阶段/终态。
5. App restart 后旧 ref unavailable；恢复 Job 新 Agent attempt 产生新 ref。
6. Task clear 不直接删除 Session；registry retention 独立。
7. Agent terminal 与 validate/publish failure 可分别判断。

## Implementation steps

1. 抽取/完善统一 Memory Agent session wrapper，四个 scope 不复制 wiring。
2. 接入 Project/Global execution paths。
3. 接入 Recall persistent/replay paths，保持其既有 tools/binding。
4. 完善 registry retention/eviction/unavailable reason。
5. 完善 Task Stage ref 更新与 retry 新 ref。
6. 实现 frontend unavailable/evicted/previous-process 视图。
7. 审计 tenant/context locator/credential/path 的公开字段。
8. 运行四 scope 矩阵与 #20/#33 回归。

## Acceptance criteria

- [ ] 四种 scope 共享同一 Session View/observer。
- [ ] Recall 多 Turn 和 replay/live 合并正确。
- [ ] tenant/credential/internal locator 无泄漏。
- [ ] retry/recovery 创建新 ref 并保持 Job 语义。
- [ ] evicted/restart ref 显示 unavailable，不伪造历史。
- [ ] Task clear 与 registry retention 解耦。

## Verification

- 四 scope Fake Agent tests；
- persistent Recall tests；
- registry capacity/shutdown tests；
- cross-tenant AppService tests；
- Task Center observer tests；
- G-02/G-03/G-04/G-09/G-11。

## Non-goals

- 修改 Memory Recipe/Evidence/Recall 产品逻辑；
- 持久化 viewer；
- Team cleanup（T11）。

## Handoff

T08 与本卡完成后 T11 进入 frontier。
