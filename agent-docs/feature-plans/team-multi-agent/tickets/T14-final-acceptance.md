# T14：完成跨入口验收与 Provider smoke

## Outcome

对父 Issue #19 做完整行为验收：Desktop、Engine、Go CLI、MCP/CLI fallback、ACP/Native、恢复和后台生命周期一致；形成可审计的关闭证据。

## Blocked by

T12、T13，以及所有 Checkpoint P0/P1 finding 已关闭。

## Read

- Contracts：01-contract.md 全部。
- Seams：02-codebase-seams.md 全部；Tests TS01–TS06。
- Gates：G0–G5，行为矩阵 V01–V18。
- 父 Issue #19 全文、全部子 Issue 完成评论和 Checkpoint review。

## Red test first

本卡先运行完整矩阵并记录真实失败，不先写生产代码。每个失败必须映射到 V-ID/Contract ID；修复只处理阻止父 Issue 验收的缺口，并先增加回归测试。

## Execution steps

1. 审计 V01–V18 的自动化证据。完成标准：每行指向真实 test 名、入口和结果；单元 mock 不能冒充 TS01。
2. 运行 G5 和 CLI E2E。完成标准：记录命令、退出码、测试数量；生成契约无意外差异。
3. 执行 deterministic full-flow fixture。完成标准：create Team→Leader chat→draft→review→multi execution→mailbox→reopen→partial restore→restart 全链路通过。
4. 执行可选真实 Provider smoke。完成标准：记录版本、capability 和 PASS/unsupported；失败不通过修改测试规避。
5. 做 privacy/authority audit。完成标准：Conversation 零写入；日志/snapshot 无正文、credential、resume token；无 Vendor switch 和直连数据库入口。
6. 生成父 Issue 验收评论。完成标准：列出 commits、矩阵、命令、migration/rollback、residual risks 和未支持 Provider。

## Acceptance

- [ ] V01–V18 每项都有可复现自动化证据。
- [ ] G5、CLI E2E、boundary 和 generated-contract checks 全部通过。
- [ ] 完整 fake ACP/Native 流程通过应用重开和重复事件验证。
- [ ] Desktop、Engine、CLI、MCP/fallback 状态与错误一致。
- [ ] Conversation/Memory/search 没有 Team 浏览副作用。
- [ ] operation logs/task snapshots 无敏感正文和恢复凭据。
- [ ] 真实 Provider smoke 的支持/不支持结果被准确记录。
- [ ] 父 Issue #19 获得自包含关闭证据。

## Non-goals

新增产品能力、顺手重构、长期 retention、云同步、Provider 私有文件适配和扩大已批准范围。

## Ticket-specific stop

验收发现架构缺口时创建或重开聚焦修复工单；不在本卡用大范围补丁掩盖。任何 P0/P1 未关闭时父 Issue 保持打开。

