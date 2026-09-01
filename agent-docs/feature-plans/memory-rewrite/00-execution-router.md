# Memory 重写：执行路由

> 面向 Luna、Flash 等执行型 Agent。每次运行只进入一张执行卡；本文件选择本轮上下文，产品规格仍以 Issue 为准。

## 1. 触发范围

处理以下任一内容时进入本执行包：

- GitHub Issue #20 或由它拆出的 Memory 子 Issue；
- 新的 Recent Work、Session/Project/Global Memory、Context Resolver；
- 新的 Recall Agent、Memory 后台 Job、Memory 投影文档；
- 旧 Dream、旧 Recall、candidate、Evidence 或 Library 的切换与删除。

通用多 Agent 对话、Team 编排、成员协作 UI 不属于本执行包。Recall 只扩展它自身所需的持久 Agent Session 能力。

## 2. 权威顺序

发生冲突时按以下顺序处理：

1. 仓库根 `AGENTS.md` 与已接受 ADR：工程和不可逆架构约束。
2. [GitHub Issue #20](https://github.com/util6/assetiweave/issues/20) 及最新评论：Memory 产品规格。
3. 当前子 Issue、blocker 和验收：本轮交付范围。
4. 代码、追加 migration、测试、生成契约与 CLI `--help`：当前已实现事实。
5. 本目录文档：执行方法、稳定契约、接缝与验证索引。

Issue 与代码不同先用失败测试证明缺口。上位来源互相冲突时执行 `05-luna-flash-playbook.md` 的 Stop protocol，不自行选择新语义。

## 3. 每轮固定入口

每个执行 Agent 按顺序读取：

1. `AGENTS.md`。
2. 父 Issue #20、当前子 Issue、最新评论和 blocker；子 Issue 尚未发布时读取 `03-ticket-map.md` 对应行。
3. 本文件。
4. 当前执行卡列出的 `01-contract.md` Contract IDs。
5. 当前执行卡列出的 `02-codebase-seams.md` Seam IDs。
6. 当前执行卡列出的 `04-verification-matrix.md` Gate IDs。
7. `05-luna-flash-playbook.md`。
8. `tickets/` 中唯一一张当前执行卡。

除 Checkpoint Review 外，不加载其他执行卡。单卡上下文是范围边界，不是建议。

## 4. 调度规则

- 一轮只执行一个 Ticket ID。
- 全部 blocker 已完成后，Ticket 才进入 frontier。
- 同一 Ticket 只允许一个 Agent/工作树写入。
- 并行 Ticket 使用不同 `codex/` 分支或 worktree。
- 每张卡执行 Locate → Baseline → Red → Minimal → Converge → Verify → Review → Commit → Handoff。
- 未取得 Red 证据前，不写生产实现。
- 提交信息使用中文 Conventional Commit；一张卡一个可回滚提交。
- Agent 只报告下一张 ready Ticket，不自动开始下一张卡。
- GitHub 子 Issue 发布后，子 Issue 的真实编号替代文档中的 TNN 调度身份；TNN 继续作为稳定执行卡编号。

## 5. 文档地图

| 文档 | 何时读取 | 用途 |
|---|---|---|
| `01-contract.md` | 每轮按 Contract ID 选读 | 不可破坏的领域、管线、Recall 与迁移约束 |
| `02-codebase-seams.md` | 每轮按 Seam ID 选读 | 生产入口、测试缝隙、legacy 清单与 Codex 参考 |
| `03-ticket-map.md` | 调度、拆 Issue、Checkpoint | 依赖图、frontier、演示结果和完成定义 |
| `04-verification-matrix.md` | 测试与交付 | 分层 Gate、行为矩阵和最终验收 |
| `05-luna-flash-playbook.md` | 每轮 | 工作卡、执行循环、停止协议、模型校正 |
| `06-handoff-template.md` | 每轮交付 | Issue 评论、提交说明和跨上下文交接 |
| `tickets/TNN-*.md` | 只读当前卡 | 本轮 Outcome、Red test、步骤、验收和非目标 |

## 6. 父功能完成条件

只有 T15 的最终验收矩阵全部通过，并且 T14 已删除或归档旧公开表面，父 Issue #20 才具备关闭证据。任何单卡通过都只代表一个垂直切片完成。
