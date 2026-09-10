# T09：接入 Session Memory 只读执行现场

## Outcome

Session Memory 后台 Agent call 注册共享 Session，Task View Agent Stage 暴露 typed ref；用户从 Task Center 打开 observer，看到实际 prompt、thinking、tools、assistant 和 terminal，且没有 Composer。

## Blocked by

T03。Issue #33 的 Task View、Memory Stage、frontend Task Center 外部前置条件已由提交 `dcb0fcbf` 满足。

## Scope

- Requirements：R-MEM-001–005、R-NFR-001–004/008
- Contracts：C-010–C-033、C-060–C-063、C-110–C-114
- Seams：S-BE-05/06/08–10、S-FE-05/06、S-TEST-01/03/05
- Gates：G-02–G-05、G-09、G-11

## Preflight

1. 读取 #33 最新 Issue/comment/router/progress，并检查 `dcb0fcbf..HEAD` 的相关 diff。
2. 以当前代码重新确认 Task View/Memory Agent Stage API；若相对本规格发生不兼容漂移，执行 Stop Protocol。
3. 记录其他任务 dirty files，禁止覆盖。
4. 以当前代码为 additive 接缝，不恢复旧 #33 设计。

## Red tests

Backend：

1. Session Memory 在 execute 前注册 ref 和真实 request item。
2. `AiExecutionRequest.progress` 连接 projection，Fake Agent events 可读取。
3. Memory Task Agent Stage 携带 ref；Task detail/items/notification 不携带 transcript。
4. Agent terminal 后 validation/publish 继续执行。
5. Agent error/cancel mark Session terminal，同时 Memory Job 遵守既有状态机。

Frontend：

1. Agent Stage 有“查看执行现场”。
2. 点击加载 shared observer 并显示 canonical items。
3. observer 无 composer/permission/write action。
4. 返回 Task 恢复选中 task/stage。
5. observer render/load failure 不改变 running task。

## Implementation steps

1. 通用化 runtime registry 的 opaque ref/get/subscribe；保留 Team compatibility address。
2. 增加 AppService/Tauri/frontend service/schema 的 Agent Session get + revision invalidation。
3. 在 Session Memory execute wrapper 中 register/request/progress/terminal。
4. 在 #33 Task View/Stage additive 增加 typed ref；不内联 items。
5. 在 Task Center 组合 observer host，复用 shared AgentSessionWorkspace mode=observer。
6. 将 Task cancel/retry 留在 Task Header；observer 只读。
7. 对 logs/tracing/task event 运行正文泄漏审计。
8. 合并后运行 #31/#33/Memory targeted tests。

## Acceptance criteria

- [ ] Session Memory 一次真实/fixture执行可从 Stage 下钻。
- [ ] observer 显示真实 request/thinking/tool/assistant/terminal。
- [ ] Task View 与 task-updated 不含 transcript。
- [ ] observer 无输入/写操作。
- [ ] validation/publish 与 Job facts 不受 viewer 影响。
- [ ] unknown/cross-tenant ref 安全 unavailable。
- [ ] 关闭 observer 不取消任务。

## Verification

- registry/AppService/Tauri tests；
- Session Memory Fake Agent integration；
- Task Center + observer component test；
- Task notification payload audit；
- desktop running/terminal observer；
- G-02–G-05、G-09、G-11。

## Non-goals

- Project/Global/Recall（T10）；
- observer 发消息；
- transcript persistence；
- 改写 #33 其他能力。

## Handoff

完成后 T10 进入 frontier。记录共用 Memory wrapper 是否足以覆盖其他 scope。
