# T07：注入受限 Team MCP 工具

## Outcome

支持 MCP 的 Leader/Teammate 可以在 Provider Session 中读取允许的任务、更新自有任务并发送/读取 mailbox；每个成员凭据只允许其角色和 owner 范围。

## Blocked by

T06。

## Read

- Contracts：C-R04、C-H04、C-T01、C-T02、C-A01、C-S01。
- Seams：S01、S03、S04、S06、S07、S09；Tests TS01、TS02、TS03。
- Gates：G0、G1、G4。

## Red test first

以两个 Teammate 身份调用 Team tool：成员 A 能读取/更新自己的任务，修改成员 B 任务必须得到稳定权限错误；无效或旧 token 被拒绝，数据库无越权变化。

## Execution steps

1. 定义最小 Team tool surface 和角色/owner policy。完成标准：每个 tool 直接映射一个 AppService workflow，无通用 SQL/命令执行入口。
2. 建立 loopback MCP host 与短期 per-member credential。完成标准：credential 不进入持久 Team 表、日志和 task detail。
3. 把 Team MCP 配置注入 ACP session new 与 resume。完成标准：进程重建后重新解析 endpoint/token/member，不复用失效凭据。
4. 补工具幂等、权限、取消和 replay guard。完成标准：历史 replay 不会再次执行 tool mutation。

## Acceptance

- [ ] Leader/Teammate 只看到角色允许的工具和数据。
- [ ] Teammate 只能修改自己拥有的 TeamTask。
- [ ] invalid/expired/cross-member credential 无持久副作用。
- [ ] new 与 resume 都获得当前 MCP 配置。
- [ ] replay 不重复 tool call、mailbox 或任务状态转换。
- [ ] Team MCP handler 只调用 AppService。

## Non-goals

公网 MCP、长期 token、任意 shell 工具、Go CLI fallback 和 Native resume。

## Ticket-specific stop

需要把 token 写入 Agent Market/Team 持久表，或让 MCP handler 直接使用 repository 时停止。
