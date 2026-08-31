# Team 多 Agent：Ticket Map

父规格：[Issue #19](https://github.com/util6/assetiweave/issues/19)。执行卡位于 `tickets/`。发布为 GitHub 子 Issue 后，以 tracker 中的真实编号和 blocker 状态为准。

| 顺序 | Ticket | Blocked by | 可验证结果 |
|---:|---|---|---|
| 1 | T01 建立 Team 与固定成员编排 | — | 创建、查看、编辑 Team roster |
| 2 | T02 建立 Persistent ACP 执行上下文 | — | 同一 context key 跨调用 resume，OneShot 不变 |
| 3 | T03 接入 Leader 主聊天与历史回放 | T01、T02 | 关闭并重开后恢复 Leader 正文 |
| 4 | T04 生成结构化草稿并进入人工审核 | T03 | 草稿可审阅，确认前零执行 |
| 5 | T05 确认并执行单个 Teammate 任务 | T04 | 用户确认映射得到严格执行和 Leader 回复 |
| 6 | T06 建立 TeamMailbox 协作闭环 | T05 | 任务板、mailbox、结果回传同源 |
| 7 | T07 注入受限 Team MCP 工具 | T06 | Agent 通过最小权限工具协作 |
| 8 | T08 提供 Engine/CLI Team 工具 fallback | T06 | 非 MCP Agent 走 CLI→Engine→AppService |
| 9 | T09 执行多任务、多 Teammate 固定计划 | T07、T08 | 多成员执行且无 fallback/移交 |
| 10 | T10 恢复完整 Team 与部分失效成员 | T03、T09 | Leader 优先、Teammate 后台、失效显式 |
| 11 | T11 建立可重启 Team 协调器 | T09、T10 | outbox/restart 后幂等继续 |
| 12 | T12 接入全局进度、轮询与退出保护 | T11 | 页面切换仍可见、遗漏事件可恢复、退出有提示 |
| 13 | T13 扩展 Native Resume 与能力准入 | T02、T08 | Native 可作为合格成员，Leader 能力单独校验 |
| 14 | T14 完成跨入口验收与 Provider smoke | T12、T13 | 全矩阵通过，形成父 Issue 关闭证据 |

## Frontier

1. 初始可并行：T01、T02。
2. T01+T02 完成：T03→T04→T05→T06。
3. T06 完成：T07 与 T08 可并行。
4. T07+T08 完成：T09→T10→T11→T12。
5. T02+T08 完成：T13 可与 T09–T12 并行。
6. T12+T13 完成：T14。

## Checkpoints

| Checkpoint | 完成 Ticket | 必须成立 |
|---|---|---|
| CP1 基座 | T01–T03 | Team 可创建；Leader Persistent chat 可恢复；Conversation 零写入 |
| CP2 人工门 | T04–T06 | 草稿、审核、单任务和 mailbox 形成完整闭环 |
| CP3 多 Agent | T07–T09 | MCP/CLI 两条工具路径收敛，多成员无移交执行 |
| CP4 恢复 | T10–T12 | restart/partial restore/progress/exit 行为闭合 |
| CP5 完成 | T13–T14 | Native 能力与全入口验收闭合 |

每个 Checkpoint 使用新上下文只做 review；发现 P0/P1 时先创建或重开修复工单，再进入后续 frontier。

