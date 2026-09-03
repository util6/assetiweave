# Team 聊天工作台：Ticket 地图

Issue #21 按依赖图拆成 15 张执行卡。每张卡完成一个可测试结果；共享核心文件的卡保持串行。

## 依赖图

```text
T01 Session Event contract
  └─ T02 semantic capability admission
       ├─ T03 ACP live bridge
       └─ T04 Antigravity anchor + live bridge
            └─ T05 Provider history replay

T03 + T04 + T05
  └─ T06 background member-turn workflow
       └─ T07 Tauri/Engine/CLI transport
            └─ T08 frontend session store
                 └─ T09 chat workspace shell
                      ├─ T10 direct member streaming
                      └─ T11 inline Leader planning
                           └─ T12 task projection + jump

T05 + T10 + T12
  └─ T13 progressive restoration
       └─ T14 UX/accessibility/responsiveness
            └─ T15 final acceptance
```

## Phase 1 — Provider Session 基础

| Ticket | Outcome | Blocked by | 主要 Gates |
|---|---|---|---|
| T01 | 通用 Session Event 与有界 transient projection | None | G0、G2、G5 |
| T02 | Resume/History/Live semantic capability 全链路与 Team 准入 | T01 | G0、G2、G3 |
| T03 | ACP live text/thought/tool 翻译与 final-text 兼容 | T01、T02 | G0、G2、G5 |
| T04 | Antigravity 真实 Conversation ID、每轮 CLI 和 live events | T01、T02 | G0、G2、G6 |
| T05 | Provider History Replay port 与 Antigravity transcript reader | T03、T04 | G0、G2、G5、G6 |

### CP1 — Provider Session Review

- 通用 event contract 不含 Team/Vendor 分支。
- ACP 与 Antigravity 均能通过 deterministic fixtures 产生 live events。
- Antigravity binding 保存真实 ID，空 ID 不覆盖有效 anchor。
- replay 无业务副作用、无正文持久化、OneShot 基线保持绿色。

## Phase 2 — 应用与 Transport 垂直路径

| Ticket | Outcome | Blocked by | 主要 Gates |
|---|---|---|---|
| T06 | AppService 后台 member turn/replay、stream snapshot 和取消 | T03、T04、T05 | G0、G2、G5 |
| T07 | Tauri event + snapshot fallback + Engine/Go CLI parity | T06 | G0、G2、G3、G4 |
| T08 | 前端 per-member Session store、schema、service 与 event reducer | T07 | G0、G1、G5 |

### CP2 — Streaming Vertical Slice Review

- 从 AppService 启动 member turn 立即返回 task/stream snapshot。
- fake Provider delta 能通过 Tauri/Engine 契约进入前端 store，重投不重复。
- 页面离开不取消执行；订阅丢失后 snapshot/polling 恢复。
- prompt、正文和 Resume Anchor 不进入 durable snapshot/log。

## Phase 3 — Chat-first 产品体验

| Ticket | Outcome | Blocked by | 主要 Gates |
|---|---|---|---|
| T09 | GoLutra 风格 Team chat shell 与头像 Session 切换 | T08 | G0、G1、G7 |
| T10 | 当前成员直聊、流式文字/思考/工具与后台持续 | T09、T06 | G0、G1、G2、G7 |
| T11 | Leader 单 composer 任务模式与内联审核卡 | T09 | G0、G1、G2、G7 |
| T12 | TeamTask 投影到 owner 时间线、Leader 聚合与跳转 | T10、T11 | G0、G1、G2、G7 |
| T13 | 结构化现场优先、活动成员优先 replay、partial/unavailable | T05、T10、T12 | G0、G1、G2、G5、G7 |
| T14 | 滚动、响应式、键盘、可见焦点和视觉一致性 | T13 | G0、G1、G7 |
| T15 | 全矩阵、桌面 smoke、独立 Review 与遗留路径清理 | T14 | G0–G8 |

## 并行边界

- T03 与 T04 在 T01/T02 完成后可用不同 worktree 并行；两者不得同时改通用 event contract。
- T09 之后的前端卡共享 Team 页面和 Session store，默认串行。
- deterministic fixture 编写可与对应生产实现并行，但合并前必须由同一卡 Agent 收敛 acceptance。
- Checkpoint Review 使用新上下文只读执行，不与生产实现 Agent 共用结论。

## Frontier 选择

1. 从最小 Ticket ID 开始。
2. 排除 blocker 未完成、已有受让者或共享文件正在被其他 worktree 修改的卡。
3. 选择剩余列表第一张。
4. 交接只报告下一张 ready 卡，不提前执行。

## 范围控制

- 单卡建议不超过 8 个 production/test 文件；预计超过 10 个时先报告切分点。
- production diff 建议小于 500 行；总 diff 超过 900 行时停止评估拆卡。
- 当前卡不创建后续卡 DTO、空组件、migration 占位或 vendor switch。
- #21 的产品验收只在 Issue 中维护；本地图只维护执行依赖。
