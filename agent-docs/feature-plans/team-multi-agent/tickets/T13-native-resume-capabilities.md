# T13：扩展 Native Resume 与能力准入

## Outcome

Native Agent 通过声明式参数恢复 Persistent context；Team 成员选择分别校验 Resume、Replay/Read 和 Team tool 能力，使可恢复但不能回放正文的 Agent 仅作为 Teammate。

## Blocked by

T02、T08。

## Read

- Contracts：C-R02–C-R06、C-H05、C-T01、C-S01、C-S02。
- Seams：S02、S05、S06、S10–S12；Tests TS02、TS03、TS04、TS05。
- Gates：G0–G4。

## Red test first

fake Native Definition 声明 resume argv 但不声明 history replay：第一次执行保存锚点，第二次按声明恢复；Team picker 允许它作为 Teammate、拒绝作为 Leader。参数记录中没有 shell 拼接、prompt、credential 或原始 env value。

## Execution steps

1. 扩展 Agent Definition/catalog 的 Native resume 和 history read/replay 能力。完成标准：安装、持久化、reload 后声明不丢失。
2. 在 Native backend 按声明生成 executable+argv。完成标准：不使用 shell command，不按 Agent ID 分支，stable cwd/anchor 正确。
3. 接入 Team member eligibility。完成标准：Leader=Resume+Replay；Teammate=Resume；tool transport 选择独立判断。
4. 把 CLI fallback 注入 Native Agent 可用上下文。完成标准：成员身份和短期 credential 不写入日志/持久 Definition。
5. 覆盖 dead anchor、unsupported、timeout、cancel 和 process reap。完成标准：不静默 fresh，不破坏 OneShot Native 行为。

## Acceptance

- [ ] Native first-use/resume 使用同一 stable binding。
- [ ] Provider 差异完全来自声明/Adapter。
- [ ] Resume-only Agent 可选为 Teammate，不可选为 Leader。
- [ ] Resume+Replay Agent 可选为 Leader。
- [ ] Native Agent 可通过受限 CLI fallback 协作。
- [ ] 自动测试只依赖 fake executable，不依赖真实 Provider。

## Non-goals

为缺少 replay 的 Provider 解析私有历史文件、修改 Conversation Adapter、Provider fork 和跨 Provider migration。

## Ticket-specific stop

Provider history 只能通过未声明、无版本契约的私有文件猜测时，不把该 Provider 标为 Leader-capable。
