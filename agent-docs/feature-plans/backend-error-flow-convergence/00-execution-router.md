# 后端错误链收口 Execution Router

> **For Luna / Flash:** REQUIRED SUB-SKILL: 使用 `superpowers:executing-plans`，严格按 `03-ticket-map.md` 一轮一张卡执行。

**Goal:** 保留 Rust 标准 `Result`、`AppResult` 与稳定 `WireError`，清除跨模块 `Result<T, String>`、错误 source 丢失和重复字符串分类。

**Issue authority:** GitHub Issue #2。

**Blocking edge:** Issue #2 被 Issue #24 阻塞；#24 关闭且 B2-G03 `VERIFIED` 前，本计划只允许读取和重放 baseline，不修改生产代码。

## 每轮步骤

1. 读取根 `AGENTS.md`、Issue #2 全部评论、本页、`01-contract.md`、当前卡。
2. 读取 `../backend-infrastructure-convergence-v2/05-execution-playbook.md`；使用同一 RED→GREEN→DELETE→Gate 协议。
3. 记录 HEAD、dirty paths 和当前卡 baseline 命中；与 dirty path 重叠时评论 `DRIFT` 并停止。
4. 选择 `03-ticket-map.md` 中第一张前置全部 `VERIFIED` 的卡。
5. 一个提交只改变一个错误 Authority；使用中文 Conventional Commit。
6. 按 `05-handoff-template.md` 评论 Issue #2 后停止。

## 不变量

- `std::result::Result`、`Ok`、`Err`、`?` 和 `AppResult<T>` 保留。
- `thiserror` 是 typed error 定义工具；不引入 eyre/error-stack 或第二套全局错误框架。
- `anyhow` 只允许位于进程最外层初始化和不进入公开协议的内部编排。
- Tauri 与 Engine 继续输出相同 `WireError { code, message, retryable, details }`。
- 公开 wire code、数据库 schema、TaskState 和现有业务成功结果保持兼容。
