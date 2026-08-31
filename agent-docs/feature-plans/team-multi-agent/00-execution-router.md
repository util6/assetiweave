# Team 多 Agent：执行路由

> 面向 Luna、Flash 等执行型 Agent。每次运行只进入一张执行卡；本文件负责选择上下文，不负责重复规格。

## 1. 权威顺序

发生冲突时按以下顺序处理，并在当前工单停止扩展：

1. 仓库根 `AGENTS.md` 与已接受 ADR：工程和不可逆架构约束。
2. [GitHub Issue #19](https://github.com/util6/assetiweave/issues/19) 及最新评论：本功能产品规格。
3. 当前子 Issue 及 blocker：本轮交付范围和验收。
4. 代码、migration、测试和生成契约：当前已实现事实。
5. 本目录文档：执行顺序、接缝、验证和交接方法。

Issue 与代码不同并不自动表示代码错误：先建立能证明目标行为缺失的失败测试。Issue、ADR 或当前子 Issue 互相矛盾时，按 `05-luna-flash-playbook.md` 的 Stop protocol 报告。

## 2. 每轮固定入口

每个执行 Agent 必须依次读取：

1. `AGENTS.md`。
2. 当前子 Issue 全文、最新评论和 blocker；尚未发布子 Issue 时读取 `03-ticket-map.md` 对应行与执行卡。
3. 本文件。
4. `01-contract.md` 中执行卡列出的 Contract IDs。
5. `02-codebase-seams.md` 中执行卡列出的 Seam IDs。
6. `04-verification-matrix.md` 中执行卡列出的 Gate IDs。
7. `05-luna-flash-playbook.md`。
8. `tickets/` 中唯一一张当前执行卡。

除 Checkpoint Review 外，不一次加载其他执行卡。后续卡片不可见可以降低提前实现和跨范围重构概率。

## 3. 调度规则

- 一轮只执行一个 Ticket ID。
- 只有全部 blocker 已关闭，Ticket 才进入 frontier。
- 同一 Ticket 只由一个 Agent/工作树写入。
- 并行 Ticket 必须使用不同 `codex/` 分支或 worktree。
- 每张卡先测试、后实现、再验证；未取得红色失败证据前不写生产实现。
- 当前卡验收全部有证据后才提交；提交信息使用中文 Conventional Commit。
- Agent 只报告下一张 ready Ticket，不自动继续。

## 4. 文档地图

| 文档 | 何时读取 | 用途 |
|---|---|---|
| `01-contract.md` | 每轮按 Contract ID 选读 | 不可破坏的领域和运行时约束 |
| `02-codebase-seams.md` | 每轮按 Seam ID 选读 | 当前生产入口、测试接缝和参考代码 |
| `03-ticket-map.md` | 调度和 Checkpoint | 依赖图、并行 frontier、完成定义 |
| `04-verification-matrix.md` | 测试和交付 | 分层门禁、证据格式、最终验收 |
| `05-luna-flash-playbook.md` | 每轮 | 工作卡、执行循环、停止和 Review 协议 |
| `06-handoff-template.md` | 每轮交付 | 工单评论和跨上下文交接格式 |
| `tickets/*.md` | 只读当前卡 | 本轮目标、步骤、验收、非目标 |

## 5. 功能完成条件

只有 `T14` 的最终矩阵全部通过，父 Issue #19 才具备关闭证据。单张 Ticket 通过仅代表一个垂直切片完成，不代表 Team 功能完成。

