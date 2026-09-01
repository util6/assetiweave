# T10：完成 Recall 多轮、取消与恢复

## Outcome

用户可以在同一 Recall Session 连续说“不是这个项目”“再早一点”，Agent 保留此前线索与范围；运行可取消，应用/Agent 退出后状态可恢复或明确失败，旧 live event 不会被重放执行。

## Blocked by

T09。需要单轮 Recall binding 和结构化结果。

## Read

- Contracts：C-A01、C-A02、C-A04、C-A05、C-R01–C-R08、C-S02、C-S03。
- Seams：S04、S06、S07、S10、S13–S16；Tests TS03、TS06。
- Gates：G0、G1、G2、G3、G7。

## Authority changed

扩展 Recall workflow 状态机、provider binding、turn execution metadata 和 active task projection；消息顺序与正文继续由 Conversation 表达。

## Red test first

Fake ACP 第一轮返回候选，第二轮用户排除项目并要求更早时间；恢复同一 provider binding 后第二轮工具参数反映累积范围。取消中的执行进入 terminal cancelled；应用重启后 completed turn 可回放显示但不再次调用工具；失效 binding 返回结构化 `resume_unavailable`。

## Execution steps

1. 明确 Recall Session/turn 状态机与顺序约束。完成标准：同 Session 单 active turn，重复 send/cancel 幂等。
2. 为 Recall 接入 Persistent provider binding 与历史恢复。完成标准：binding 属于共享 Agent runtime，Recall 只保存 context key/reference。
3. 区分 replay 与 live events。完成标准：回放只构造 UI/DTO，不执行工具、不写新 turn。
4. 实现 cancel、Agent process exit、resume unavailable 和 retry UX。完成标准：状态在 TaskRuntime 与 SQLite 之间可重建且不互相冒充 Authority。
5. 完成多轮 UI。完成标准：连续提问、运行状态、取消、错误恢复、follow-up suggestion 都可操作，页面导航不阻塞。

## Acceptance

- [ ] 多轮保留先前线索与用户收窄条件。
- [ ] 同 Recall Session 不并发执行两个 turn。
- [ ] 取消与重复取消幂等。
- [ ] 重启/回放不重复调用工具或生成结果。
- [ ] provider binding 失效返回结构化错误，不静默新建冒充恢复。
- [ ] 进程退出、超时与权限拒绝均有可恢复 UI。
- [ ] 未引入通用 Team/多 Agent UI。

## Non-goals

使用反馈、Engine/CLI、旧页面删除、通用 Persistent Agent 产品。

## Ticket-specific stop

如果恢复只能靠把完整 ACP 历史复制到 Memory 表，或 replay 会触发 live side effects，停止并报告。
