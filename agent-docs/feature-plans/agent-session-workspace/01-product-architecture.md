# 产品与架构规范：共享 Agent Session 工作区

## 1. 文档状态

- 父规格：GitHub Issue #31
- 关联：Issue #19、#20、#21、#33
- 状态：Accepted for implementation
- AionUi 视觉基准：commit `18022a49684d5a2b54b0a47f904e76b17f758b3b`
- 记录日期：2026-09-10

## 1.1 规范语言

- **必须 / MUST**：实现与验收不可省略；不满足即本规格未完成。
- **应 / SHOULD**：默认实现；只有代码事实或上位约束冲突时可偏离，并在 handoff 记录理由。
- **可以 / MAY**：不影响合同的实现选择。
- 文档中的陈述性合同条款默认按 MUST 解释；示例名称允许按仓库惯例调整，语义与职责不允许调整。

## 2. 问题定义

当前 Team UI 只能聚焦一个成员，页面结构以大标题和嵌套卡片为主，无法高密度地观察并行 Agent。当前 Session item 又把工具事件压入通用文本，ACP 映射与前端 sanitizer 丢弃工具输入输出，因而 UI 不能像 AionUi 一样完整展示 Turn、Thinking、Tool Steps 与结果。

Task Center 能回答 Memory Job 处于领取、加载、Agent、校验或发布阶段，但不能显示 Agent 调用的执行现场。用户需要从 Memory 的 Agent Stage 下钻，使用与 Team 单聊一致的只读界面查看本次 request、thinking、assistant、tools、error 与 terminal。

在 Team 与 Memory 分别实现时间线会产生两套合并状态机、两套渲染器和两个事实源。目标是一个共享、领域中立的 Agent Session 工作区。

## 3. 产品术语

| 术语 | 定义 | 明确区别 |
|---|---|---|
| Agent Session | 一次可寻址的 Agent 执行现场；可以是 one-shot 或 persistent | 不是 Conversation Session 持久资产 |
| Agent Session Ref | 当前进程内访问 Session View 的 typed opaque capability | 不是数据库 ID，不跨进程稳定 |
| Agent Session View | Session Event 经合并、排序、有界化后的公开只读投影 | 不是 Provider 原始协议或 Task View detail |
| Turn | 一次用户/系统请求及其 assistant、thinking、tool、terminal 的有序集合 | 不等于 Conversation Turn 持久事实 |
| Tool Step | 一次逻辑工具调用的 start/update/result 合并投影 | 不等于一个顶层 Task |
| Interactive | 允许发送并按 capability 执行 stop/queue/interrupt 的 Chat 模式 | 不代表所有 AionUi backend 功能都已实现 |
| Team | Interactive Session 的 Team 领域组合：成员、parallel/single、Task/Plan | 不拥有消息渲染器 |
| Observer | Memory 使用的只读 Session 工作区 | 无 composer，不改变 Job |
| Task View | TaskRuntime 的业务阶段、Activity 与终态审计 | 不保存 Agent transcript |

## 4. 核心架构决策

### A-001：共享视图优先

先建立 `AgentSessionWorkspace`，Team 与 Memory 作为组合者接入。共享层依赖 Session View + capabilities + callbacks，不依赖 Team、Memory、TaskRuntime 或 ACP。

### A-002：一个执行现场 Authority

`SessionEventProjection` 继续负责事件去重、顺序、合并、bounds 与 snapshot。现有 runtime `SessionStreamRegistry` 从 Team-specific key 通用化为 Agent Session registry；不建立新 registry。

### A-003：AppService 唯一业务边界

AppService 提供 Session 查询、合法操作路由与 Task ref 关联。Tauri 只映射 transport，前端只调用 services。

### A-004：Task 与 Session 分层

Task View 保持业务阶段摘要；可选 `agentSessionRef` 仅为下钻链接。Agent Session items 不复制到 TaskRuntime。Agent terminal 与 Memory publish 是两个不同事实。

### A-005：进程内、有界、非持久

Session View 仅在当前应用进程可用。关闭应用或 registry 淘汰后返回 unavailable。完整执行现场不进入 SQLite、Conversation、Memory、Search、operation log 或 tracing。

### A-006：AionUi 行为参考，AssetIWeave 技术实现

复刻 AionUi 的信息架构、视觉比例、状态与交互。实现只使用本项目 React 19、TypeScript、Rust、Tauri、Foundation/Common、semantic Theme Token 与 frontend services。

### A-007：能力驱动，而非领域条件驱动

共享工作区根据 capability 显示 send、stop、retry、queue、interrupt、attachment、mention、slash、model、permission、copy、openArtifact。Memory observer 的能力集合始终不包含写操作。

### A-008：事实展示

只展示 Provider/Agent runtime 实际提交或返回的事实。缺失显示 unavailable/partial；截断显示 truncated；processing 不伪造成 thinking。

### A-009：Expand–Migrate–Contract

1. Expand：在旧 Team DTO/组件旁加入共享 View 与兼容 adapter；
2. Migrate：Tool、Timeline、Chat、Team、Memory 逐个切换；
3. Contract：所有消费者迁移后删除旧 Team-specific 呈现和 sanitizer。

## 5. 产品模式

### 5.1 Interactive

必须包含：

- 紧凑 Session Header；
- 完整 Timeline；
- Turn/Thinking/Steps/Terminal；
- 底部 Composer；
- capability 驱动的 send/stop/queue/interrupt；
- loading/empty/restoring/error/unavailable；
- auto-follow、new activity、scroll-to-latest；
- 复制与可用 Artifact 打开动作。

本规格不新增独立“单聊”导航。Interactive 组件由 Team member lane 真实使用，未来普通 Agent 单聊可以直接组合。

### 5.2 Team

在 Interactive 上增加：

- Team rail/header；
- Member tabs、Leader 标识、状态、未读；
- parallel/single；
- 每 lane 独立 timeline、scroll、composer；
- Leader task mode、Plan、TeamTask；
- pause/stop/interrupt/retry 与成员恢复。

### 5.3 Observer

复用相同 Timeline/Turn/Thinking/Steps renderer，但：

- 无 composer；
- 无 permission response；
- 无直接 retry/cancel Agent call；
- Header 明确“只读执行现场”；
- Task cancel/retry 留在 Task Center；
- Session unavailable 时仍保留 Task View 阶段证据。

## 6. 功能需求

### Session 与数据

- **R-SES-001**：每个 Session View 具有进程内唯一 `sessionRef`、execution identity、tenant scope、purpose、agent/model、mode、state、revision。
- **R-SES-002**：同一 event identity 只应用一次；同一 item 的 start/update/result 合并。
- **R-SES-003**：Turn 内真实顺序不因 UI 分组改变。
- **R-SES-004**：Live 与 Replay 使用相同模型；同 identity 冲突时更高 sequence 优先，相同 sequence 时 live 优先。
- **R-SES-005**：Terminal 状态单调，不被旧 running snapshot 覆盖。
- **R-SES-006**：超限数据显式 truncated；registry 释放显式 unavailable。

### 聊天与步骤

- **R-CHAT-001**：用户与 assistant 呈现层次对齐 AionUi。
- **R-CHAT-002**：Markdown、代码、链接、列表和可复制文本可读。
- **R-CHAT-003**：thinking delta 原位增长；processing-only 显示状态。
- **R-CHAT-004**：连续 tools 聚合为 `查看步骤 · N`；后续文本关闭分组。
- **R-CHAT-005**：Step 支持 Input、Output、Diff、Location、Error、Exit、Image/Artifact。
- **R-CHAT-006**：运行组默认展开，回放完成组默认折叠，用户选择不被流式更新覆盖。
- **R-CHAT-007**：在底部时自动跟随；上滚后冻结并提示新活动。

### Team

- **R-TEAM-001**：Leader 首位，其他成员按 `sort_order`。
- **R-TEAM-002**：宽屏多成员默认 parallel；lane 不低于可读宽度，超出横向滚动。
- **R-TEAM-003**：single 显示 active member；切换不丢各 lane scroll。
- **R-TEAM-004**：其他 lane 流式更新不抢当前 lane 焦点或滚动。
- **R-TEAM-005**：每 lane composer 的接收者明确且独立。
- **R-TEAM-006**：Plan/TeamTask/ownership/review/dispatch 保持既有 Authority。

### Memory

- **R-MEM-001**：Session/Project/Global/Recall Agent call 均可注册 Session View。
- **R-MEM-002**：Task View 只携带 typed opaque ref，不内联 items。
- **R-MEM-003**：点击 Agent Stage 打开 observer，显示本次实际 request、thinking、assistant、tools、terminal/error。
- **R-MEM-004**：observer 无 composer，无写操作。
- **R-MEM-005**：Agent terminal 后的 validate/publish 仍显示在 Task View。
- **R-MEM-006**：重启/淘汰后的 ref 返回 unavailable；恢复 Job 创建本进程新 ref。

### 非功能需求

- **R-NFR-001**：所有页面数据经过 frontend services。
- **R-NFR-002**：所有业务编排经过 AppService。
- **R-NFR-003**：默认 Session bounds 保持 256 items、2048 events、4 MiB。
- **R-NFR-004**：高频 delta 合并，避免 token 级全工作区重渲染。
- **R-NFR-005**：颜色、边框、阴影使用 semantic tokens。
- **R-NFR-006**：320/768/1024/1440 像素验收。
- **R-NFR-007**：键盘、屏幕阅读器、reduced-motion 达到现有项目门禁。
- **R-NFR-008**：模型内容不进入 Debug/tracing/task notification。

## 7. 非目标

- 新增 Agent transcript SQLite 表；
- 把 Session View 变成 Conversation Session；
- 改写 Memory Job/lease/retry/last-success；
- 改写 TaskRuntime retention、Adapter concurrency 或通知；
- 引入 AionUi runtime dependencies；
- 复制 AionUi Explorer、SCM、Office、Browser、Cron 等业务子系统；
- 为不存在的 backend capability 展示假控件；
- 从结果反推隐藏 reasoning 或缺失工具过程；
- 新增独立单聊产品导航。

## 8. 成功判定

功能完成时，用户可以：

1. 在 Team 同时看到多个成员独立而完整的实时聊天现场；
2. 聚焦任意成员，查看 request、thinking、steps、tool details 和 answer；
3. 从 Memory Task Agent Stage 打开同一风格的只读执行现场；
4. 区分 Agent 已返回与 Memory 已校验/发布；
5. 在重新进入页面、事件重连和历史 replay 时不看到重复条目；
6. 在超限或现场释放时看到明确事实，不看到伪造的完整历史。
