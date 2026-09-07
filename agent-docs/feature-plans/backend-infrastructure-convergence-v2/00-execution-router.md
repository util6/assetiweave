# 后端基础设施生态收口第二阶段 Execution Router

> **2026-09-06 审计纠偏结论：** 审计后纠偏卡 B2-R16 至 B2-G03 已全部实施并验收通过。
> GitHub Actions CI [Run 34050901408](https://github.com/util6/assetiweave/actions/runs/34050901408) 5 个 jobs 100% 成功。
> 详情见 [`08-remediation-router.md`](08-remediation-router.md) 与 [`10-remediation-verification-matrix.md`](10-remediation-verification-matrix.md)。
> Issue #24 正式关闭。

> **For agentic workers:** REQUIRED SUB-SKILL: 使用 `superpowers:executing-plans`，一次执行一张卡。只有维护者明确安排并行工作时才使用 `superpowers:subagent-driven-development`。

**Goal:** 在保留 AssetIWeave 领域语义的前提下，让 Tokio、tokio-util、SQLx、Serde、tracing 与经验证的进程/路径 crate 成为通用基础设施的唯一生产实现。

**Architecture:** `AppRuntime` 是进程级资源与关闭协调 Authority；`Database` 最终只拥有 `SqlitePool`；ResidentHost 与 OneShot 复用 async-first `AppService` workflow。Task、durable outbox 与 portable path anchors 保留产品语义，调度和 OS 机制交给成熟生态。

**Tech Stack:** Rust 1.96、Tokio、tokio-util、SQLx、Serde、thiserror、tracing；候选依赖与锁定版本见 `01-contract.md`。

**Spec:** [GitHub Issue #24](https://github.com/util6/assetiweave/issues/24)

## 每轮唯一入口

执行者每轮严格按以下顺序工作：

1. 读取根 `AGENTS.md`、Issue #24 及其全部评论。
2. 读取本页与 `03-ticket-map.md`，从 Issue 最新交接确认前置项全部 VERIFIED 的第一张卡。
3. 只读取该卡声明的 Contract IDs、`07-codebase-seams.md` 对应分支和 `05-execution-playbook.md`。
4. 运行该卡的 Preflight；记录 HEAD、工作树、目标符号和测试现状。
5. 若代码与卡片的输入契约不一致，按“漂移停止条件”提交证据，不修改生产代码。
6. 先增加或确认能反驳旧 Authority 的行为测试，观察预期 RED 或记录为何现有测试已经覆盖。
7. 切换真实生产 consumer；删除旧机制或收敛为卡片允许的单向委托。
8. 运行卡片 Gate 与 `G-RUST-BASE`；失败时只在本卡范围内诊断和修复。
9. 按 `06-handoff-template.md` 向 Issue #24 写交接评论并提交一个中文 Conventional Commit。
10. 停止。下一张卡必须由新一轮执行重新读取状态。

## 状态 Authority

- **实现事实：** 当前代码、测试、Cargo 配置与生成契约。
- **计划状态：** Issue #24 最新交接评论；本目录表格只保存依赖顺序，不持续回写完成状态。
- **历史上下文：** #1、#2、#22 及旧 feature plan。历史文本不能推翻当前代码，也不能作为完成证据。
- **完成证据：** 行为测试 + production consumer 切换 + 删除证据 + Gate 输出，四项缺一不可。

## 全局约束

- 默认串行；一轮只认领一张卡、一个 Authority、一个提交。
- 保留用户已有未提交修改；Preflight 发现脏工作树时，记录路径并只修改卡片声明文件。路径重叠即停止。
- 不恢复旧实现来匹配过期计划，不创建 `legacy`、`new`、`v2` 平行运行时。
- `asset_mounts`、TaskState/Dedup/Conflict、durable outbox/offset、portable path anchors 与来源只读规则保持不变。
- Tauri、Engine 和 CLI 保持 `frontend services -> Tauri/Engine -> AppService -> backend -> SQLite/filesystem` 边界。
- 生成契约只通过 `pnpm cli:contract` 更新；不手工编辑生成文件。
- 依赖加入必须同时删除或明确简化生产机制；安装成功不计完成。
- 公共错误的 `code/message/retryable/details` 保持兼容，内部诊断继续脱敏。
- 每个行为变化先测试后实现；源码字符串守卫只证明旧结构被删，不替代行为验收。
- 每张卡结束时工作树只包含该卡预期变更；中文 commit 示例：`refactor(runtime): 统一事件派发器生命周期`。

## 漂移停止条件

出现任一情况，本轮只在 Issue #24 留下 `DRIFT` 评论后停止：

1. 卡片点名的核心符号、生产 consumer 或测试 seam 已不存在，且无法直接映射到同一 Authority。
2. 目标文件与开始前用户未提交修改重叠。
3. 前置卡没有 VERIFIED 交接，或最新交接报告 Gate 失败。
4. 需要改变 Issue #24 的产品不变量、公开 wire contract 或新增数据库迁移。
5. 候选 crate 无法在 Rust 1.96 或任一支持平台满足卡片行为 fixture。
6. 单卡预计需要同时改变两个以上独立 Authority；应先修订拆分，不直接扩大范围。

`DRIFT` 评论必须列出：当前 HEAD、卡片 ID、预期符号、实际符号、最小复现命令、影响和建议改卡方式。

## 结束条件

只有 `B2-G01` 的全部 Gate 通过、Issue #24 中所有前置交接均为 VERIFIED，才可声明第二阶段完成。此时运行时、进程、设置、日志、路径、SQLx 和错误清理均具有行为与删除证据；任一项只有依赖或类型存在时，整体保持未完成。
