# T02：建立 Persistent ACP 执行上下文

## Outcome

同一个 `execution_context_key` 的两次 ACP 执行复用可恢复 Provider Session；进程可以回收，Session 和绑定保留；现有 OneShot 行为完全不变。

## Blocked by

None。#18 的 OneShot 行为是回归基线，不把 Issue 状态当作已实现证据。

## Read

- Contracts：C-R01–C-R06、C-A02、C-S01、C-S02。
- Seams：S02、S03、S04、S06；Tests TS02、TS03。
- Gates：G0、G1、G4。

## Red test first

fake ACP 第一次执行返回 Session ID；第二次以同 key 执行时必须观察到 typed resume/load 而不是第二个 session/new。执行结束后 process 已 reap、稳定 workspace 和 binding 仍存在、Provider Session 未 delete。

## Execution steps

1. 扩展协议无关 request 和 binding repository。完成标准：context key 与 execution ID 分离，binding 可在 runtime 重建后读取。
2. 让 ACP backend 按 binding 选择 new 或 typed resume/load。完成标准：能力不支持、dead anchor、timeout 都返回结构化错误。
3. 分离 Persistent terminal cleanup 与 OneShot cleanup。完成标准：Persistent 回收进程但保留 Session/binding；所有 OneShot 回归测试原样通过。
4. 把 Resume 能力和必要声明贯穿 catalog→installation→reload→runtime。完成标准：无 Agent ID 分支。

## Acceptance

- [ ] 首次执行原子保存恢复锚点后才报告 ready/success。
- [ ] 后续执行使用同一 Provider Session 和稳定 workspace。
- [ ] 每次执行仍产生新的 execution ID。
- [ ] dead anchor 返回 `resume_unavailable`，不静默新建。
- [ ] Persistent 普通终态不删除 Provider Session/binding。
- [ ] #18 OneShot 成功、失败、取消、超时和 cleanup 测试全部通过。

## Non-goals

Team 表、Leader UI、历史 Replay、Native resume、MCP 和进程池。

## Ticket-specific stop

ACP SDK typed API 与预期不同时，贴出准确 capability/type/signature；生产代码需要 raw JSON-RPC 或 Vendor switch 时停止。
