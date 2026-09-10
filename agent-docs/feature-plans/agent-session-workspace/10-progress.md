# 执行进度：Agent Session Workspace

> 本文件是本执行包唯一进度表。Issue/PR 状态仍以 GitHub 为准；这里记录稳定 TNN 映射与验证证据，不复制完整实现说明。

## 当前状态

- Parent Issue：#31
- Related：#20、#33
- Phase：Specification ready
- Initial frontier: T10
- 外部前置条件：Issue #33 Task View/Memory Stage 基线已由 `dcb0fcbf` 满足

## Ticket Progress

| ID | GitHub | Status | Commit | Gates | Note |
|---|---|---|---|---|---|
| T01 | — | verified | 72c856e3 | G-00, G-04, G-11 | 抽取共享 Agent Session 兼容工作区 |
| T02 | — | verified | 12f8cd3 | G-01, G-04, G-05, G-11 | 打通一条完整 Tool Step |
| T03 | — | verified | 9a3eb4cd | G-01, G-02, G-04, G-05, G-11 | 完整 Turn/Thinking/View Steps 还原与冲突单调性 |
| T04 | — | verified | c8a8604d | G-04, G-06, G-10, G-11 | AionUi 风格单聊 Header/Composer/自动滚动与状态机 |
| T05 | — | verified | d182f26c | G-01, G-05, G-07, G-11 | 补齐终端与命令结果展示、ANSI过滤、退出码与截断 |
| T06 | — | verified | b06cdea1 | G-05, G-07, G-10, G-11 | 补齐文件、Diff、图片与 Artifact 展示 |
| T07 | — | verified | 865f4942 | G-06, G-08, G-10, G-11 | 实现 Team 多成员 Parallel / Single 工作区 |
| T08 | — | verified | eb12d537 | G-06, G-08, G-11 | 迁移 Team Plan、Task 与成员运行操作 |
| T09 | — | verified | 90e46823 | G-02, G-03, G-04, G-05, G-09, G-11 | 接入 Session Memory 只读执行现场与 Task Center 观察者 |
| T10 | — | ready | — | — | frontier；解阻，可执行 |
| T11 | — | blocked | — | — | blocked by T08 + T10 (等待 T10) |
| T12 | — | blocked | — | — | blocked by T05/T06/T08/T10/T11 |

## Checkpoints

| Checkpoint | Status | Evidence |
|---|---|---|
| CP-1 Shared Contract | verified | T01–T03 Canonical Fixture、全量 155 前端测试与 949 Rust 测试全部通过 |
| CP-2 Chat + Team | verified | T04, T07, T08 验证完成，前台构建与 161 前端测试、949 Rust 测试全绿 |
| CP-3 Memory Observer | pending | — |
| CP-4 Final | pending | — |

## 更新规则

- 子 Issue 发布后填入 GitHub 编号；
- commit 后填 hash；
- Gate 只记录通过的 Gate ID，失败详情放 handoff/Issue；
- 状态只使用 ready/running/blocked/verified；
- blocker 完成后更新 frontier；
- 不在本表粘贴测试大段输出。
