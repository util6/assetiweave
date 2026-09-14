# Memory Q1–Q36 SPEC：入口、Authority 与执行协议

- 状态：Accepted；生产实现待执行
- 日期：2026-09-15
- 产品规格：[Issue #35](https://github.com/util6/assetiweave/issues/35)
- 架构决策：[ADR-0015](../../../adr/0015-memory-hybrid-authority-and-generation-skill.md)
- 适用对象：Luna、Flash 及其他单上下文代码执行 Agent

## 1. 本套文档的用途

这是一套面向执行 Agent 的目标状态规范。它描述 Issue #35 的最终行为，不描述当前实现已经具备的行为。Agent 发现代码与本规范不一致时，应先写失败测试证明差距，再实现目标；不得把当前行为反写成规格。

本目录采用渐进披露。每轮只读取本文件、`01-product-contract.md` 和当前任务需要的专题文件。完整加载全部文件会稀释当前切片的完成边界。

## 2. Authority

目标行为发生冲突时按以下顺序判断：

1. 根 `AGENTS.md`、已接受 ADR 的工程红线与安全边界；
2. Issue #35 与 ADR-0015；
3. 本目录的稳定 Contract ID、状态机、Schema 和验收条件；
4. Issue #20/#30 中与前三项不冲突的有界证据、短引用、Durable Job、last-success、脱敏和 ACP 隔离要求；
5. 当前代码和测试，仅用于确认迁移起点；
6. `memory-rewrite` 目录中的旧 T01–T15/E0–E8 文档，仅作为已完成历史与既有测试缝参考。

已明确失效的旧语义包括：固定滚动 72 小时、默认项目视图、逐项目 Markdown、来源删除后级联删除长期条目、用户可编辑 Recipe、Recent 页面刷新按钮和 Recent 内容节点级跳转。

## 3. 文档地图

| 分支 | 必读文件 | 完成判据 |
|---|---|---|
| 任何 Memory #35 工作 | `00-index.md`、`01-product-contract.md` | 当前切片引用了适用的 `M35-*` Contract ID |
| 领域模型、migration、repository | `02-domain-and-storage.md` | 表约束、身份、revision、引用生命周期和迁移阶段均有测试 |
| Recent Snapshot、调度、续接 | `03-l1-snapshot-pipeline.md` | 一个目标水位从候选选择到 last-success 发布可重复验证 |
| L2/L3、Context、长期修订 | `04-l2-l3-promotion-context.md` | 准入、低频 Consolidation、来源失效和 supersede 可重复验证 |
| 生成 Skill、ACP、工具和输出 | `05-generation-skill-acp.md` | Skill 版本固定、权限不扩张、结构化输出准入成立 |
| AppService、Engine、Tauri、CLI、Task | `06-appservice-and-task-contracts.md` | AppService 是唯一业务实现且所有公开表面同构 |
| Markdown、Recent UI、统一设置 | `07-markdown-ui-settings.md` | 两份只读投影与时间优先页面从同一 Snapshot 得到结果 |
| 迁移、测试、安全、发布 | `08-migration-testing-and-rollout.md` | 目标 Gate 和行为矩阵有可复现证据 |
| 工作拆分或新任务开工 | `09-execution-slices.md` | 只选择 frontier 中一张未完成切片 |

## 4. 固定执行协议

每张实现工单按以下顺序执行：

1. **Locate**：读取当前 Issue、blocker、适用 Contract ID、现有生产入口和既有测试先例。
2. **Baseline**：记录 `git status --short`，区分用户已有修改；运行最小目标测试并确认至少执行一个测试。
3. **Red**：通过 AppService 主测试缝写外部行为测试，观察目标缺口导致失败。
4. **Minimal**：只实现当前切片；所有持久化状态变化经过 AppService，数据库只用追加 migration。
5. **Converge**：同步 Tauri/Engine/CLI/frontend service 中当前切片实际改变的公开合同；生成文件只通过生成命令更新。
6. **Verify**：运行当前切片 Gate、`git diff --check` 和契约一致性检查。
7. **Review**：逐项核对 Contract ID、越权面、last-success、租户隔离、日志正文泄漏和晚到结果。
8. **Commit**：使用中文 Conventional Commit；一张切片形成一个可独立回滚的提交。
9. **Handoff**：记录提交、测试命令、未覆盖风险和下一 frontier；当前 Agent 不自动进入下一张切片。

完成判据：当前工单的所有验收条件可由命令或桌面观察复现，且没有通过占位返回、跳过测试或读取旧投影伪造成功。

## 5. 当前实现基线

截至本规范日期，仓库已有：

- canonical Conversation、Session Memory、Project Memory、Global Memory；
- 可恢复 Memory Job、TaskRuntime 投影、Fake AgentExecutor 和可控时钟测试先例；
- 有界证据首包、短引用、补读预算和引用校验；
- `memory.recent.list` 的 72 小时 Session/Event 投影；
- Project/Global Markdown 与 Context Resolver；
- 两个 Memory 页面、任务中心及独立 Recall；
- `MemoryRecipe` 与 Recipe snapshot。

这些是迁移起点。目标实现新增结构化 Recent Snapshot 与三层 Memory Item，并逐步退出旧 72 小时 Recent、逐项目文档和 Recipe 新任务路径。

## 6. 固定工程命令

```bash
git status --short
git diff --check
cargo fmt --all -- --check
cargo test --workspace
pnpm typecheck
pnpm test
pnpm build
go vet -C cli ./...
go test -C cli -race ./...
pnpm check:boundaries
pnpm test:boundaries
pnpm cli:contract
pnpm check:surface-matrix
```

目标测试先于全仓命令执行。CLI Contract 连续生成两次必须字节一致；生成时使用临时数据库，避免触碰用户数据库。

## 7. 全局完成定义

只有同时满足以下条件，Issue #35 才达到实现完成：

- `M35-AUTH-*`、`M35-L1-*`、`M35-L2-*`、`M35-L3-*`、`M35-SKILL-*`、`M35-API-*`、`M35-PROJ-*`、`M35-UI-*`、`M35-OPS-*` 全部有自动化证据；
- SQLite 可在没有 Markdown 的情况下完整读取和重建当前 Memory；
- 每个 tenant 只产生两份新 Markdown；
- 02:00/14:00、24/48/72、无变化复用、续接和 missed-watermark 均由可控时钟验证；
- Session 删除不会删除已晋升 L2/L3，且不可用引用停止跳转；
- 用户 Skill 可修改总结策略但不能扩大能力；
- Recent 页面只允许展开、投影切换和 Session 导航；
- 旧 `memory.recent.list`、Recipe 新任务路径和逐项目文档写入完成 contract 阶段退出；
- 全仓 Gate 通过，桌面 smoke 无 P0/P1 缺陷。
