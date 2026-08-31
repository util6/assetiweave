# T08：提供 Engine / CLI Team 工具 fallback

## Outcome

不能接收 Team MCP 的 Agent 可以通过受限 Go CLI 命令完成同样的任务板和 mailbox 操作；所有命令经 Engine 进入 AppService，与 Desktop/MCP 行为一致。

## Blocked by

T06。

## Read

- Contracts：C-A01、C-T01、C-T02、C-S01。
- Seams：S01、S10、S11；Tests TS01、TS04。
- Gates：G0、G1、G3、G4。

## Red test first

CLI-to-Engine 测试以成员 A credential 更新自有任务成功，更新成员 B 任务失败；AppService 临时数据库出现与 MCP 路径相同的状态和错误码。

## Execution steps

1. 为现有 Team AppService workflows 注册最小 Engine methods 和风险/确认语义。完成标准：DTO 与 MCP policy 使用同一输入校验。
2. 生成 contract，并实现 Go client/commands。完成标准：Go 代码不导入 SQLite、不操作 Team 文件、不复制状态机。
3. 为 provider fallback 定义非交互安全输出。完成标准：结构化结果可由 Agent 消费，stdout/stderr 不包含 credential 或正文日志副本。
4. 覆盖权限、幂等、not-found 和 stale revision。完成标准：CLI/MCP/Desktop 公开错误一致。

## Acceptance

- [ ] CLI 可以读取自有任务、更新状态、发送和读取允许的 mailbox。
- [ ] 所有 CLI mutation 经 Engine→AppService。
- [ ] 与 MCP 相同身份的允许/拒绝结果一致。
- [ ] generated contract 由 `pnpm cli:contract` 更新。
- [ ] credential、resume token 和正文不会进入命令诊断日志。

## Non-goals

Native Provider 启动参数、Agent 安装、Team UI、通用 admin CLI 和直接数据库工具。

## Ticket-specific stop

如果 Engine contract 无法表达成员身份而需要 CLI 自行授权，停止并修改共享 AppService/DTO 设计。
