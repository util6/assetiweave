# 任务一：成熟依赖替换与契约统一 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: 使用 `superpowers:executing-plans`，一次执行一张卡；只有明确安排并行执行时使用 `superpowers:subagent-driven-development`。步骤以 checkbox 跟踪。

**Goal:** 让成熟 React/Rust 库接管通用机制，切换真实生产调用方，删除旧实现，再完成跨层契约统一。
**Architecture:** 保留 React → services → Tauri/Engine → AppService → SQLite/文件系统。允许领域模块整体重写，以行为回归和单一权威为边界，不先建设新插件平台。
**Tech Stack:** 保留现有核心栈；新增/复用依赖及精确版本见 [02-dependencies.md](02-dependencies.md)。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。本目录是其执行拆解，不是另一份产品规格。

## Global Constraints

- 本目录只执行**任务一**；插件架构属于独立的 [任务二](../plugin-architecture-refactor/00-execution-router.md)，不在本任务顺便实现。
- Node 22（新 ESLint 至少 22.13）、pnpm 10、Go 1.24、Rust 至少 1.96.0；保留 React/Vite/TS/Tailwind/shadcn/Radix/Zod/dnd-kit/TanStack Virtual/Vitest/Tauri/Tokio/Serde/SQLite。
- 内部接口、目录和模块均可重写；保留已确认业务结果、数据、来源只读、直接软链接默认策略、后台任务响应性及 CLI 能力。
- Memory/Team 在施工范围内；已有未提交成果是输入，不是待清理文件。
- 不新增 Playwright/桌面 E2E 框架、不扩展 AI 数据迁移产品、不做独立视觉改版。
- 卡片是执行单位；共享 package/lock/AppProviders/AppService/Engine registry 的卡串行。默认串行，不要求执行模型自行派生 Agent。

## 固定读取顺序

1. 根 `AGENTS.md`；`gh issue view 22 --comments`。
2. 本页 → [03-ticket-map.md](03-ticket-map.md)，选择所有前置项已验收的唯一一张卡。
3. 当前卡列出的 `01-contract.md` Contract IDs；只读取有关条款。
4. 安装依赖时读取 `02-dependencies.md` 对应行；定位源码时读 `07-codebase-seams.md` 对应行。
5. [05-playbook.md](05-playbook.md) → 当前卡 → [04-verification-matrix.md](04-verification-matrix.md) 对应 Gate。
6. 结束时用 [06-handoff-template.md](06-handoff-template.md) 写 Issue 交接评论。

当前状态：**全部卡 PLANNED，未实施。** 首卡为 [A00](tickets/A00-baseline.md)。写了计划、安装了包、执行了少量测试，均不等于本任务完成。

## 权威与冲突

- Issue 保存用户目标/范围；代码、测试、生成契约保存实现事实；本目录约定施工顺序和技术接口。
- ADR 保留其业务目标，不把 ADR0012 历史表述当成全部已实现的证明。ResidentHost 后台快照与 OneShotEngine 执行后退出的差异见 C-TASK。
- 代码相对计划漂移时先报告具体符号、差异和影响，修订当前卡后继续；执行模型不通过恢复旧代码“对齐文档”。
- 本包基于 2026-09-03、HEAD `6d07632` **加当时工作区修改**编制。A00 必须重新核对，不能只检出该提交当作完整基线。

## 结束与交给下一任务

A-G01 的全部门禁通过、审查无阻断项后，任务一独立交付。交接记录包含代码 revision、锁文件、统一契约、删除清单、测试证据和仍保留的真实业务机制。此时才可开启任务二的 B00；不要自动继续实现插件宿主。
