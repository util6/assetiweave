# Memory 重写：Ticket 地图

本图是把 Issue #20 转换为可独立验证的执行顺序。GitHub 子 Issue 发布后，在本表补充真实编号与 blocker；不改变 TNN 稳定编号。

## 1. 依赖图

```text
T01 Recent source slice + project resolution
  └─ T02 Session Memory from Conversation commit
       ├─ T03 Durable background recovery
       ├─ T04 Project Memory consolidation
       │    └─ T05 Global Memory + Context Resolver
       └─ T06 Invalidation propagation
            ├─ T07 Recent UI + exact navigation
            └─ T08 Hybrid retrieval + read-only tools
                 └─ T09 Recall one-turn vertical slice
                      └─ T10 Recall multi-turn/cancel/recovery
                           └─ T11 Usage feedback + retention/settings core

T03 + T05 + T06 + T10 + T11
  └─ T12 Engine/CLI/Skill public surface
       └─ T13 Desktop/settings cutover
            └─ T14 Legacy archive and removal
                 └─ T15 Full integration acceptance
```

## 2. Ticket 一览

| ID | 演示结果 | Blocked by | 主要证据 | 执行卡 |
|---|---|---|---|---|
| T01 | AppService 在可控 72h 窗口按项目目录列出多宿主 Session | — | TS01 | `tickets/T01-recent-source-slice.md` |
| T02 | Conversation commit 自动生成幂等 Session Memory 与六类事件 | T01 | TS01、TS02 | `tickets/T02-session-memory.md` |
| T03 | Job 在启动、lease 过期、retry 和取消后可恢复 | T02 | TS02 | `tickets/T03-durable-background.md` |
| T04 | 同项目不同 Agent Session 合并为 last-success Project Memory | T02 | TS01 | `tickets/T04-project-memory.md` |
| T05 | 轻量 Global Memory 与预算化 Context Resolver 非阻塞返回 | T04 | TS01 | `tickets/T05-global-context.md` |
| T06 | 修订、删除、缺失、迁目录与合同升级传播失效和重建 | T03、T04 | TS01、TS02、TS04 | `tickets/T06-invalidation.md` |
| T07 | 「近期」项目/时间视图、Markdown 与精确跳转可用 | T03、T04、T06 | TS06 | `tickets/T07-recent-ui.md` |
| T08 | Recall 工具完成过滤+lexical+semantic+rerank 并只读 | T06 | TS04 | `tickets/T08-hybrid-retrieval.md` |
| T09 | 用户发起一次 Recall，得到 answer 与可跳转结构化引用 | T08 | TS01、TS03、TS06 | `tickets/T09-recall-one-turn.md` |
| T10 | Recall 可连续追问、取消、恢复并处理 Agent 退出 | T09 | TS03、TS06 | `tickets/T10-recall-multiturn.md` |
| T11 | 实际使用写 usage；生成/使用开关与排除输入生效 | T05、T10 | TS01、TS03 | `tickets/T11-usage-governance.md` |
| T12 | Engine/CLI/Skill 只暴露新 Memory 合同且行为一致 | T03、T05、T06、T10、T11 | TS05 | `tickets/T12-engine-cli-skill.md` |
| T13 | Desktop 只剩两个页面，设置和全局任务状态完整接入 | T05、T07、T10、T11、T12 | TS06 | `tickets/T13-desktop-cutover.md` |
| T14 | 旧数据有只读归档，Dream/旧 Recall/Library/candidate/Evidence 表面退出 | T12、T13 | TS07、TS05、TS06 | `tickets/T14-legacy-removal.md` |
| T15 | 全链路、重启、响应性、权限与迁移矩阵通过 | T14 | TS01–TS07 | `tickets/T15-final-acceptance.md` |

## 3. Frontier 与并行

- 初始 frontier：T01。
- T02 完成后：T03 与 T04 可并行；T06 等待两者。
- T06 完成后：T07 与 T08 可并行。
- T10 与 T05 都完成后仍需先完成 T11，公共表面才能在 T12 一次收口。
- T14 前保持新旧底层 schema 可同时存在，但用户与公共接口切换只在 T12/T13 后统一删除；不创建长期 v1/v2 路由。
- 并行 Agent 不得同时修改同一个生成 contract、router、settings schema 或 migration 文件。

## 4. Checkpoints

### CP1 — Derived Memory foundation

完成：T01–T03。

通过条件：

- 72h/project resolution 有可控时钟证据；
- Outbox durable enqueue、revision 幂等、Session Memory 生成成立；
- Job 可从 restart/lease/retry 恢复；
- 仍未依赖 Memory 页面触发。

### CP2 — Hierarchy and freshness

完成：T04–T06。

通过条件：

- Session → Project → light Global 层级成立；
- Context 读取 last-success 并遵守预算；
- 修订/删除/迁目录/合同升级完成级联失效；
- SQLite 可独立重建 Markdown 与索引投影。

### CP3 — User workflows

完成：T07–T11。

通过条件：

- 「近期」回答最近做了什么并能回现场；
- Recall 以持久多轮 Agent 运行，工具只读，输出结构化；
- 使用反馈、开关、排除和 redaction 有行为证据；
- UI 未暴露内部 ID、locator、Evidence 或原始 Markdown。

### CP4 — Cutover

完成：T12–T14。

通过条件：

- Engine、CLI、Skill、Tauri、frontend 使用新合同；
- UI 只剩两个子页面；
- 旧数据只读归档存在，旧公开方法和自动路径退出；
- 不保留平行规则引擎。

### CP5 — Release evidence

完成：T15。

通过条件：`04-verification-matrix.md` 的 Gate G0–G8 与 V01–V24 全部有可复现证据。

## 5. Ticket 完成定义

一张 Ticket 只有同时满足以下条件才完成：

1. 当前卡所有 Acceptance 勾选且有测试/命令证据。
2. 当前卡列出的 Contract IDs 未被破坏。
3. diff 未实现后续 Ticket 的占位业务。
4. 生成物由命令生成，用户已有修改保持原样。
5. 一条中文 Conventional Commit 可单独回滚。
6. `06-handoff-template.md` 已记录提交、验证、剩余风险和下一 frontier。
