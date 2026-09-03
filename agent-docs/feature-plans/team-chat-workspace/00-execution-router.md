# Team 聊天工作台：执行路由

> 面向 Luna、Flash 等执行型 Agent。本目录是 Issue #21 的执行包，不是第二份产品规格。每轮只读取并完成一张执行卡。

## 1. 权威顺序

发生冲突时按以下顺序判断，并按 `05-luna-flash-playbook.md` 停止扩展：

1. 仓库根 `AGENTS.md` 与已接受 ADR：工程、领域和不可逆架构约束。
2. [GitHub Issue #21](https://github.com/util6/assetiweave/issues/21) 及最新评论：本阶段产品规格。
3. [GitHub Issue #19](https://github.com/util6/assetiweave/issues/19)：已落地 Team 领域和执行基线；只在 #21 未改变的范围继续有效。
4. 当前代码、migration、测试、CLI `--help` 和生成契约：当前实现事实。
5. 本目录：切片、接缝、门禁和交接方法。

Issue 与代码不同先写失败测试证明缺口。#21 与 #19 对前端展示或成员历史能力存在差异时，以 #21 为当前阶段目标，不回写旧文档。

## 2. 每轮固定入口

执行 Agent 依次读取：

1. `AGENTS.md`。
2. Issue #21 全文与最新评论。
3. 本文件。
4. `01-contract.md` 中当前卡列出的 Contract IDs。
5. `02-codebase-seams.md` 中当前卡列出的 Seam IDs。
6. `04-verification-matrix.md` 中当前卡列出的 Gate IDs。
7. `05-luna-flash-playbook.md`。
8. `tickets/` 中唯一一张当前执行卡。

Checkpoint Review 才读取同一阶段已完成的多张卡。普通执行不预读后续卡，防止提前实现和跨卡重构。

## 3. 调度规则

- 一轮只执行一个 Ticket ID；一个提交只表达该卡的一个可验证结果。
- Blocker 全部满足后才进入 frontier。
- 同一 Ticket 只允许一个 Agent/工作树写入；共享核心文件的卡保持串行。
- 每张卡执行 Locate → Baseline → Red → Minimal → Converge → Verify → Review → Commit → Handoff。
- 生产代码必须由当前卡最高层失败测试牵引；低层测试不能替代公开行为证据。
- 当前卡完成后只报告下一张 ready Ticket，不自动继续。
- 提交信息使用中文 Conventional Commit；保留用户已有未提交修改。

## 4. 文档地图

| 文档 | 何时读取 | 用途 |
|---|---|---|
| `01-contract.md` | 每轮按 ID 选读 | 不可破坏的 UI、Session、恢复和 Authority 约束 |
| `02-codebase-seams.md` | 每轮按 ID 选读 | 生产入口、最高层测试接缝和参考证据 |
| `03-ticket-map.md` | 调度与 Checkpoint | 依赖图、并行边界和完成顺序 |
| `04-verification-matrix.md` | Red/Verify/交付 | 分层门禁和证据格式 |
| `05-luna-flash-playbook.md` | 每轮 | 工作卡、执行循环、停止协议和 Prompt |
| `06-handoff-template.md` | 每轮结束 | 提交与下一上下文交接 |
| `tickets/TNN-*.md` | 只读当前卡 | 当前 Outcome、步骤、验收和非目标 |

## 5. 阶段完成条件

只有 T15 完成最终矩阵、独立 Review 无阻断项，且 Issue #21 的 P0 行为均有自动化或桌面证据，本阶段才具备关闭条件。单卡通过不代表聊天工作台完成。
