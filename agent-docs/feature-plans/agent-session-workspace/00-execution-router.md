# Agent Session 工作区：执行路由

> 面向 Luna、Flash 及其他执行型 Agent。本执行包落地 GitHub Issue #31，并与 Issue #20、#33 的 Memory/Task Center 接缝协作。每次执行只加载当前分支所需文档和一张工作卡。

## 1. 目标

本专项交付一个领域中立、协议无关的 Agent Session 工作区：

1. 在 AssetIWeave 技术栈内复刻 AionUi 的完整聊天信息架构和交互；
2. Team 每个成员复用同一 interactive chat surface，多成员支持 parallel/single；
3. Memory Task 的 Agent Stage 通过 typed ref 打开同一工作区的 read-only observer；
4. 展示真实 request、assistant、thinking、processing、Tool Step、Diff、终端结果、错误和 terminal；
5. 继续使用 AppService、AgentExecutionRuntime、SessionEventProjection、TaskRuntime 与既有持久化 Authority，不建立平行运行时或 transcript 数据库。

## 2. 触发范围

处理以下任一内容时进入本执行包：

- Issue #31 或它拆出的子 Issue；
- Agent Session View、Session Stream Registry、Session Event/Item 公开投影；
- AionUi 聊天界面复刻、完整 Turn、Thinking、`查看步骤 · N`、Tool Step 详情；
- Team member chat、parallel/single lanes、Team chat composer；
- Memory Agent Session observer、Task View `agentSessionRef`；
- Team 专用 Session 呈现向共享 Agent Session 呈现的迁移与收缩。

纯 Task Center 列表、Conversation Adapter 并发、任务通知和设置继续进入 Issue #33 执行包。纯 Memory Job/Recipe/Evidence/Recall 产品逻辑继续进入 Issue #20 执行包。

## 3. 权威顺序

发生冲突时按下列顺序处理：

1. 根 `AGENTS.md` 与 Accepted ADR；
2. GitHub Issue #31 的最新正文与评论；
3. 交叉领域分别遵守 Issue #20、#33；
4. 当前子 Issue、blocker 与验收标准；
5. 代码、测试、生成契约与 CLI `--help`；
6. 本执行包。

代码与 Issue 的当前事实不一致时，先用 Red Test 固定缺口。上位来源互相冲突时执行 Stop Protocol，不自行选择新产品语义。

## 4. 必须保持的 Authority

| Authority | 唯一事实源 | 本专项允许的动作 |
|---|---|---|
| Agent 执行 | `AgentExecutionRuntime` | 增加/丰富展示事件，不建立第二执行器 |
| 进程内执行现场 | `SessionEventProjection` + runtime registry | 通用化地址与公开投影，保持有界、非持久 |
| Team 结构化事实 | AppService + SQLite TeamRun/TeamTask | 只改变呈现与引用，不改变角色/审核/ownership |
| Memory Job | Issue #20 的 SQLite Job/lease/retry/last-success | 关联只读 Session ref，不改变 Job 状态机 |
| Task Center | `TaskRuntime` + AppService Task View | 增加 typed ref，不内联 Session transcript |
| Provider 历史 | Provider-owned Session/history | 可回放为 Session items，不复制到 Conversation/Memory |
| 前端数据访问 | `frontend/src/services` | 新增共享 service/provider，组件不直调 Tauri |
| UI 设计系统 | Foundation/Common + semantic Theme Tokens | 翻译 AionUi 行为，不引入其运行时依赖 |

## 5. 每轮固定读取顺序

每个执行 Agent 按顺序读取：

1. 根 `AGENTS.md`；
2. 当前 GitHub 子 Issue、父 Issue #31、最新评论和 blocker；
3. 本文件；
4. `01-product-architecture.md` 中与当前卡相关的 Requirement IDs；
5. `02-public-contract.md` 中与当前卡相关的 Contract IDs；
6. `03-codebase-seams.md` 中与当前卡相关的 Seam IDs；
7. `04-ui-interaction-spec.md` 或 `05-integration-spec.md` 中当前分支；
8. `06-verification-matrix.md` 中当前 Gate IDs；
9. `07-flash-playbook.md`；
10. `tickets/` 中唯一一张当前执行卡。

除 Checkpoint/Final Review 外，不加载其他工作卡。当前卡是范围边界。

## 6. 分支读取路由

| 当前工作 | 必读 |
|---|---|
| Session DTO、事件、registry、AppService | 01、02、03、05、06、07 |
| 聊天 Shell、Timeline、Composer | 01、02、03、04、06、07 |
| Team parallel/single/workflow | 01、03、04、05、06、07 |
| Memory observer/Task Center ref | 01、02、03、05、06、07；再读 #20/#33 router |
| Legacy 收缩 | 01、02、03、05、06、07 |
| 最终视觉/全仓验收 | 01–08 全部 |

## 7. 调度规则

- 一轮只执行一个 Ticket ID。
- 全部 blocker 已完成后才进入 frontier。
- 同一 Ticket 只允许一个 Agent/工作树写入。
- 不同 Ticket 并行时使用不同 `codex/` 分支或 worktree。
- Issue #33 基线实现已落在提交 `dcb0fcbf`；若后续又存在该专项未提交施工，本专项不 reset、checkout、stash、stage、覆盖或混入该工作。
- 每卡执行 `Locate → Baseline → Red → Minimal → Converge → Verify → Review → Commit → Handoff`。
- 未取得 Red 证据前，不改生产实现。
- 一张卡一个可回滚提交；提交信息使用中文 Conventional Commit。
- Agent 完成当前卡后只报告下一个 frontier，不自动继续。
- GitHub 子 Issue 发布后，真实 Issue 编号替代 TNN 调度身份；TNN 保留为稳定文档编号。

## 8. 并发图

```text
T01 → T02 → T03
             ├─ T04 → T07 → T08 ─┐
             ├─ T05               ├─ T11 → T12
             ├─ T06               │
             └─ T09 → T10 ────────┘
```

- T04、T05、T06 在 T03 后可并行。
- Issue #33 的 Task View/Memory Stage 基线已由 `dcb0fcbf` 落地；T09 只等待 T03，开工时仍须重新验证当前接缝。
- T07 只依赖 T04；T08 只依赖 T07。
- T11 等待 Team 与全部 Memory scope 完成迁移。
- T12 等待全部用户可见能力和 Legacy 收缩完成。

## 9. 文档地图

| 文档 | 用途 |
|---|---|
| `01-product-architecture.md` | 产品范围、术语、架构决策、需求 ID |
| `02-public-contract.md` | Rust/transport/TypeScript 公开 DTO 与合并状态机 |
| `03-codebase-seams.md` | 当前事实、生产入口、测试缝隙、AionUi 参考图 |
| `04-ui-interaction-spec.md` | Chat、Thinking、Steps、Composer、Team 布局与可访问性 |
| `05-integration-spec.md` | AppService、runtime、Team、Memory、Task Center 数据流 |
| `06-verification-matrix.md` | 自动化 Gate、行为矩阵、视觉验收与完成门禁 |
| `07-flash-playbook.md` | 单卡执行协议、漂移处理、审查与 Stop Protocol |
| `08-ticket-map.md` | T01–T12 依赖、Outcome、Contract/Seam/Gate 映射 |
| `09-handoff-template.md` | 每卡提交与跨上下文交接模板 |
| `10-progress.md` | 唯一进度表，不在其他文档复制状态 |
| `tickets/TNN-*.md` | 单上下文执行卡 |

## 10. 父功能完成条件

只有同时满足以下条件，Issue #31 才具备关闭证据：

- Team 通过共享 chat surface 达到 AionUi 的 parallel/single 工作区和完整消息步骤体验；
- interactive surface 真实承载 Team member chat，不是未使用的未来抽象；
- Session/Project/Global/Recall Memory 均可从 Task Agent Stage 打开只读 observer；
- Task View 未内联 transcript，Memory observer 无 composer；
- Tool input/output、Thinking、Diff、Terminal、错误和截断均有真实端到端证据；
- 旧 Team 专用消息呈现和相反的 tool sanitizer 已收缩；
- `06-verification-matrix.md` 的 Final Gate 全部通过；
- T12 完成并给出视觉基准、全仓验证和回归报告。
