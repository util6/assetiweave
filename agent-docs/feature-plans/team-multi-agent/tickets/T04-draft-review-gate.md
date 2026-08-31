# T04：生成结构化草稿并进入人工审核

## Outcome

用户向 Leader 提交工作后，Leader 根据冻结 roster 生成结构化 TeamTask 草稿和推荐分配；界面进入待审核，用户可调整映射和顺序，确认前 Teammate 执行为零。

## Blocked by

T03。

## Read

- Contracts：C-L01、C-L03、C-P01、C-P02、C-P04、C-A01、C-A04、C-S01。
- Seams：S01、S02、S07、S09、S10、S12–S14；Tests TS01、TS04、TS05。
- Gates：G0–G4。

## Red test first

用记录所有 runtime request 的 fake runtime 启动 TeamRun：Leader 返回合法草稿后状态必须是 awaiting-review，任务和推荐 owner 已持久化，Teammate request 数量仍为零。

## Execution steps

1. 建立 TeamRun/TeamTask 状态机、roster snapshot 和结构化草稿验证。完成标准：自由文本、未知 member、空任务和重复 task identity 不能进入 awaiting-review。
2. 建立 draft background workflow。完成标准：请求快速返回 snapshot，Leader 完成后原子持久化草稿和状态。
3. 暴露 review read/update API、Engine/CLI 和 UI。完成标准：用户可改 owner/顺序；页面明确区分推荐值与当前审核值。
4. 应用 roster mutation guard。完成标准：drafting/awaiting-review 期间成员配置不可改变。

## Acceptance

- [ ] Leader 输入包含冻结 roster、Agent、模型和 Teammate 顺序。
- [ ] 合法草稿持久化为 TeamTask 并进入 awaiting-review。
- [ ] 用户可以修改每个任务的 Teammate 和顺序。
- [ ] 所有可执行任务必须引用冻结 roster 中的 Teammate。
- [ ] awaiting-review 前后直到确认，Teammate execution count 为零。
- [ ] 页面切换后 review draft 仍可读取。

## Non-goals

确认后的 dispatch、mailbox、MCP、自动排序算法、额度/网络探测和任务依赖 DAG。

## Ticket-specific stop

结构化输出必须靠解析不受约束的自然语言或重新调用 Conversation Adapter 时停止；应回到 typed/validated draft contract。

