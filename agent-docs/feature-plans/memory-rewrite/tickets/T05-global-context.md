# T05：生成轻量 Global Memory 与 Context Resolver

## Outcome

成功 Project Memory 推进轻量 Global Memory；`memory.context.resolve` 在指定项目与预算下立即返回 last-success 的 Global Summary、Project Memory 和少量相关 Session Memory，并携带 revision/时间/内部引用。

## Blocked by

T04。需要可靠 Project last-success。

## Read

- Contracts：C-A01–C-A05、C-P05、C-P07、C-P09、C-M02–C-M06、C-S02、C-S03。
- Seams：S01、S04–S06、S08、S16；Tests TS01。
- Gates：G0、G1、G2、G6、G7。

## Authority changed

新增 Global Memory/version、全局文档投影和 Context revision；Context 响应不成为新的持久事实源。

## Red test first

先生成成功 Project/Global v1，再启动会失败的 v2 重建；并发调用 Context Resolver 仍立即返回 v1 revision。给定小预算时按 Global→Project→Session 优先级截断，结果不超过预算估算并保留结构完整性。

## Execution steps

1. 建立 Global Memory/version 与 project index schema。完成标准：内容只接受稳定跨项目信号，项目细节留在 Project。
2. 实现轻量串行 Global consolidation 与 last-success。完成标准：只消费成功 Project Memory，重复 watermark 幂等。
3. 投影全局 `memory_summary.md`、`MEMORY.md`。完成标准：原子替换、损坏可重建、失败保留前版。
4. 实现 AppService Context Resolver。完成标准：项目目录规范化、可选 query 与 token budget 决定 Global/Project/Session 选择。
5. 定义稳定 context revision 与 source references。完成标准：相同输入/last-success 返回相同 revision，生成中不阻塞。

## Acceptance

- [ ] Global 仅含跨项目偏好、通用方式和项目索引。
- [ ] Context 在后台生成中/失败时读取 last-success。
- [ ] 返回 text、revision、generated time 和内部 references。
- [ ] 预算优先级为 Global→当前 Project→相关 Session。
- [ ] 文档损坏可由 SQLite 重建。
- [ ] Resolver 不等待模型、不触发同步 consolidation。

## Non-goals

自动向外部 Agent Session 注入、usage 写入、Recall、多轮 UI、Engine/CLI。

## Ticket-specific stop

如果必须把所有项目正文塞入 Global，或 Context read 需要等待当前 Job 完成，停止并报告。
