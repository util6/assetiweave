# SPEC 文档集：ACP Agent Execution Runtime

> Phase 1: OpenCode Translation

| 字段 | 值 |
|---|---|
| 状态 | Implemented |
| 版本 | 0.2.0 |
| 日期 | 2026-08-13 |
| 主仓库基线 | AssetIWeave `f409803` |
| 参考仓库基线 | AionCore `4bfd7e2cd6d6b6b3371e0b99525143cefc554c86` |
| 首个消费者 | Conversation Card Translation |
| 首个 Agent | OpenCode |
| 首个协议后端 | ACP over stdio |

## 1. 文档集目的

本目录把 ACP Agent Execution Runtime 拆成可独立评审、可逐项实现、可逐项验证的文档。执行模型不得只读取本索引后直接编码；必须按任务指定的最小上下文读取对应文档。

## 2. 文档清单与阅读顺序

| 顺序 | 文档 | 内容 | 主要读者 |
|---|---|---|---|
| 1 | [`01-product-requirements.md`](./acp-agent-execution-runtime/01-product-requirements.md) | 目标、范围、规范性需求、非功能需求、验收标准 | 产品、架构、执行者 |
| 2 | [`02-architecture-design.md`](./acp-agent-execution-runtime/02-architecture-design.md) | 分层、模块、依赖、数据契约、生命周期、错误模型 | 架构与后端执行者 |
| 3 | [`03-aioncore-reference-code-map.md`](./acp-agent-execution-runtime/03-aioncore-reference-code-map.md) | AionCore/OpenCode 具体借鉴代码、借什么、不借什么 | 调研与后端执行者 |
| 4 | [`04-acp-process-runtime-design.md`](./acp-agent-execution-runtime/04-acp-process-runtime-design.md) | Managed Process、ACP connection、session flow、取消清理 | Rust 后端执行者 |
| 5 | [`05-translation-task-api-integration.md`](./acp-agent-execution-runtime/05-translation-task-api-integration.md) | Translation、AppService、Tauri、Engine、CLI、后台任务与前端 | 全栈执行者 |
| 6 | [`06-test-verification-acceptance.md`](./acp-agent-execution-runtime/06-test-verification-acceptance.md) | 测试矩阵、fake ACP、平台进程测试、质量门 | 测试与实现执行者 |
| 7 | [`07-implementation-plan.md`](./acp-agent-execution-runtime/07-implementation-plan.md) | 按依赖排序的增量任务、每项文件范围和验证命令 | 任务编排者 |
| 8 | [`08-lunna-execution-playbook.md`](./acp-agent-execution-runtime/08-lunna-execution-playbook.md) | 低成本执行模型逐任务工作协议、输入模板、交付格式 | Lunna/执行模型 |
| 9 | [`09-progress.md`](./acp-agent-execution-runtime/09-progress.md) | 当前任务状态、验证证据、决策、阻塞与下一步 | 所有参与者 |

## 3. 规范优先级

发生冲突时按以下顺序处理：

1. 仓库根 `AGENTS.md` 与项目架构约束。
2. `01-product-requirements.md` 的 MUST / MUST NOT。
3. `02-architecture-design.md` 的组件边界与依赖方向。
4. `04-acp-process-runtime-design.md` 的生命周期和清理不变量。
5. `05-translation-task-api-integration.md` 的公开接口兼容要求。
6. `06-test-verification-acceptance.md` 的验收证据。
7. `07-implementation-plan.md` 的任务拆分。

参考项目代码是设计证据，不覆盖 AssetIWeave 自身需求。

## 4. 核心决策摘要

| ID | Proposed 决策 |
|---|---|
| D-001 | 业务依赖 `AiExecution`，不依赖 ACP。 |
| D-002 | `AgentExecutor` 只按 `AgentDefinition.protocol` 路由，不按 Vendor 路由。 |
| D-003 | Phase 1 每次 execution 启动独立 ACP process，不建 pool。 |
| D-004 | OpenCode 使用 `opencode acp`；实际翻译禁止继续使用 `opencode run`。 |
| D-005 | Phase 1 Registry 使用代码内置定义，不新增数据库表。 |
| D-006 | Translation 使用 app-owned 空临时 workspace、空 MCP、自动拒绝 permission。 |
| D-007 | 用户指定 model 时必须成功应用，不允许静默回退。 |
| D-008 | Desktop 使用后台任务快照；Engine 同步等待但共享同一 executor。 |
| D-009 | Gemini Phase 1 保留 legacy CLI seam，不允许新 Agent 使用该 seam。 |
| D-010 | 标准 ACP 优先；没有真实兼容性证据时不实现 dialect shim。 |

## 5. 实现前必须确认的决策

以下问题会改变代码范围。进入 `07-implementation-plan.md` 的对应任务前必须得到评审结论：

1. **Gemini 是否同阶段迁移**：推荐否，保留短期 compatibility seam。
2. **Desktop 首版是否包含 cancel 与全局任务提示**：推荐是，符合长耗时任务规范。
3. **Translation 是否严格禁止所有工具**：推荐是。
4. **是否使用空临时 workspace**：推荐是。
5. **Agent Registry 是否首版持久化**：推荐否。
6. **是否保留短期 OpenCode ACP feature flag**：实施中未建立双执行路径；T22 真实 smoke 通过后确认直接完成 ACP 切换。

未收到相反结论时，执行模型必须采用推荐值，不得自行扩大范围。

## 6. Phase 1 完成定义

只有同时满足以下条件才算完成：

- OpenCode Translation 端到端经过 Registry、AgentExecutor、ACP Backend 与 Managed Process。
- initialize、session/new、可选 model、prompt、cancel、close 均有 typed SDK 实现或明确能力判断。
- success、failure、timeout、cancel 均无残留 process tree。
- Translation 只聚合当前 session 的 assistant text。
- permission 自动拒绝，tool activity 按既定 policy 终止。
- Desktop 后台任务不阻塞无关 UI，并可通过 event + polling 恢复状态。
- Engine Translation command 继续工作，Gemini 不回归。
- fake ACP、process tree、AppService、frontend、Engine/CLI 测试通过。
- 日志中不存在 prompt、翻译结果、认证信息和环境变量值。

## 7. 参考代码位置

参考代码已更新到：

```text
~/fork-code/AionCore
```

OpenCode 本地参考位于：

```text
~/fork-code/opencode
```

精确文件、函数、行区间和借鉴边界见 `03-aioncore-reference-code-map.md`。

## 8. 文档维护规则

- 架构决策变化：先更新 01/02，再更新 07。
- ACP wire 或 SDK 变化：更新 03/04/06。
- 公开 DTO 或 command 变化：更新 01/05/06，并执行 `pnpm cli:contract`。
- 任务完成：在 07 标记并附验证证据，不以“代码看起来完成”代替测试。
- 每次执行开始和结束：同步更新 `09-progress.md` 的更新时间、当前任务、验证结果与下一步。
- 文档中的基线 commit 变化后，必须重新核对 03 中的代码行与结论。
