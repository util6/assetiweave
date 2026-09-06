# 后端基础设施 v2 审计后纠偏 Router

> **For Luna / Flash:** REQUIRED SUB-SKILL: 使用 `superpowers:executing-plans`。一轮只执行一张卡；
> 完成提交和 Issue 交接后立即停止。

**Goal:** 修复 2026-09-06 审计反驳的 v2 Contract，并在当前 HEAD 获得可重放的 macOS、Linux、Windows 验收证据。

**Status authority:** GitHub Issue #24 最新评论。

**Evidence:** `09-post-audit-baseline.md`。

**Gate:** `10-remediation-verification-matrix.md`。

## 每轮固定步骤

1. 读取根 `AGENTS.md`、Issue #24 全部评论、本页、当前卡和 `05-execution-playbook.md`。
2. 运行 `git rev-parse HEAD` 与 `git status --short`；记录所有开始前 dirty path。
3. 从下表选择前置项全部 `VERIFIED` 的第一张卡。不得跳卡，不得并行修改共享文件。
4. 运行卡片 Preflight；输出的每个生产命中必须分类为 `migrate`、`delete`、`test-only` 或 `retain`。
5. 先写能在当前旧机制上失败的行为测试并记录 RED；测试过滤器执行 0 tests 计失败。
6. 只修改卡片列出的文件，切换生产 consumer，再删除旧机制。
7. 运行卡片 Gate、`G-RUST-BASE` 和删除查询；所有命令 exit 0 才提交。
8. 使用中文 Conventional Commit；按 `06-handoff-template.md` 评论 Issue #24，然后停止。

## 串行队列

| 顺序 | Card | 唯一结果 | 前置 | 状态 |
|---:|---|---|---|---|
| 1 | [B2-R16](tickets/B2-R16-strict-shutdown-deadline.md) | dispatcher 与 runtime 严格共享一个绝对 deadline | 审计基线 | **VERIFIED** |
| 2 | [B2-S02](tickets/B2-S02-settings-authority.md) | backend settings 只读当前 Runtime/SQLite snapshot | B2-R16 | **VERIFIED** |
| 3 | [B2-P03A](tickets/B2-P03A-process-consumers.md) | 非 Conversation 同步进程 consumer 全部 async 化 | B2-S02 | **VERIFIED** |
| 4 | [B2-P03B](tickets/B2-P03B-conversation-process-consumers.md) | Conversation 同步进程 consumer 全部 async 化 | B2-P03A | **VERIFIED** |
| 5 | [B2-P03C](tickets/B2-P03C-delete-sync-supervisor.md) | 删除手工同步 supervisor 并强化防回退守卫 | B2-P03B | **VERIFIED** |
| 6 | [B2-L02](tickets/B2-L02-production-log-redaction.md) | tracing 成为唯一生产日志入口且路径/敏感字段脱敏 | B2-P03C | **VERIFIED** |
| 7 | [B2-F02](tickets/B2-F02-lossy-path-decisions.md) | 已确认的 lossy 身份、比较和持久化决策归零 | B2-L02 | **VERIFIED** |
| 8 | [B2-D02](tickets/B2-D02-stable-sqlx-rows.md) | 五个最高残余 store 的稳定 row typed 化 | B2-F02 | **VERIFIED** |
| 9 | [B2-G02](tickets/B2-G02-current-head-windows.md) | 当前提交通过三平台 CI 与 Windows Job Object fixture | B2-D02 | **VERIFIED** |
| 10 | [B2-G03](tickets/B2-G03-final-reacceptance.md) | Contract 重新验收、状态与证据同步 | B2-G02、Issue #1 release gate 通过 | **VERIFIED** |

全部纠偏卡与最终重新验收已全部闭环完成。
Issue #2 的错误链收口必须在 B2-G03 后执行，避免与 B2-D02、B2-L02 修改相同模块。

## 硬停止条件

- 当前卡与开始前 dirty path 重叠。
- 需要改变公开 WireError code、数据库 migration、`asset_mounts` 或 portable anchor 格式。
- 需要修改下一张卡拥有的 Authority 才能让当前测试通过。
- 卡片列出的生产命中没有全部完成处置。
- 本地缺少对应平台时把平台测试写成“已通过”。本地只能记录 `NOT RUN`，由 CI 完成验证。

出现停止条件时，以 `Status: DRIFT` 评论 Issue #24，附当前 HEAD、命令、首个失败和最小拆卡建议；不提交生产改动。
