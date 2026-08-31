# Team 多 Agent：不可变执行契约

本文件只保存跨 Ticket 稳定的约束。具体 DTO、表字段和组件名称由实现确定；执行卡通过 Contract ID 引用本文件。

## 领域边界

### C-D01 — 独立领域

Team 是独立领域，规范术语为 `Team`、`TeamMember`、`TeamRun`、`TeamTask`、`TeamMailboxMessage`。Conversation 继续只表达第三方历史记录的导入、规范化、检索和展示。

### C-D02 — 两种角色

一个 Team 恰有一个 `Leader` 和至少一个 `Teammate`。角色集合不扩展为 Planner、Reviewer、Worker 等额外领域角色。

### C-D03 — 双 Authority

SQLite 保存 Team 结构化现场；各 Provider Session 保存成员私有上下文。Leader Provider Session 还负责主聊天旧正文。两者组合形成可恢复 Team，彼此不复制内容。

### C-D04 — Conversation 零写入

Team 创建、聊天、执行、恢复和浏览不会创建或修改 Conversation Session、Turn、Part、Question、QuestionTurn 或 Content Node 事实。

## Team 生命周期

### C-L01 — 成员快照

TeamRun 创建时冻结成员、Agent、模型和 Teammate 顺序。Team 配置只在没有待审核或执行中的 TeamRun 时可修改。

### C-L02 — 稳定成员上下文

每个 TeamMember 拥有唯一稳定的 `execution_context_key`；重复使用相同 Agent/模型的两个成员仍使用不同 key。每次调用使用新的 `execution_id`。

### C-L03 — 明确状态机

TeamRun 至少区分 drafting、awaiting-review、executing 和 terminal。恢复状态属于运行投影，不建立第二套 TeamRun 事实状态机。

## 计划与执行

### C-P01 — 结构化草稿

Leader 根据冻结成员快照生成可验证的 TeamTask 草稿和每个任务的推荐 Teammate。自由文本不能直接进入 awaiting-review。

### C-P02 — 一次人工门

有效草稿进入 awaiting-review。用户可以调整 Task→Teammate 映射和顺序；确认前 Teammate 执行次数必须为零。

### C-P03 — 确认后固定

确认原子保存用户审核结果并进入 executing。当前 TeamRun 不再自动修改任务所有者。

### C-P04 — 无动态资源决策

系统不探测额度、不计算模型智力排名、不按网络质量调度。Teammate 顺序只用于推荐和展示，不是故障转移链。

### C-P05 — 无移交

任务失败、取消、断线或 Session 失效时仍归原 Teammate。没有自动 B/C fallback、自动重试或自动重新分配。

## Agent Execution

### C-R01 — 复用现有 Runtime

Team 通过现有 `AgentExecutionRuntime` 执行成员，不建立 Team-specific runtime、`AgentTurnRuntime` 或前端执行器。

### C-R02 — Persistent 语义

`AgentSessionMode::Persistent` 表示一次执行结束后保留 Provider Session 和恢复绑定。它不表示操作系统进程常驻，也不改变 execution purpose 的标签性质。

### C-R03 — Runtime 拥有绑定

Agent Execution 基础设施持久化 Provider Session ID、恢复方式、稳定 workspace、Agent/安装身份、模型、绑定版本和不透明 Provider 元数据。Team 只保存 `execution_context_key`。

### C-R04 — 能力驱动

ACP/Native 差异由能力声明和 Adapter/Definition 处理。Team 与通用执行编排不按 Agent ID 或 Vendor 名称写分支。

### C-R05 — 失效显式

恢复锚点失效时返回结构化 `resume_unavailable`。Runtime 不在同一绑定下静默新建 Session 并伪装为恢复成功。

### C-R06 — OneShot 回归锁

#18 定义的 OneShot 删除、错误和 cleanup 行为保持不变。启用 Persistent 不改变任何现有一次性调用的外部副作用。

## 主聊天与恢复

### C-H01 — Leader 正文来源

主聊天是用户与 Leader Provider Session 的对话。重新打开 Team 时从该 Session 动态恢复旧正文，不持久化 AssetIWeave-owned Team transcript。

### C-H02 — 临时展示投影

Leader 历史回放只构造当前界面的临时时间线。它不写入 Team、Conversation、Memory、搜索或 operation log。

### C-H03 — Teammate 私有历史

Teammate Session 在后台恢复上下文；其历史回放不进入主聊天。

### C-H04 — 回放无副作用

恢复回放与新 live event 明确区分。回放不会再次执行工具、创建 mailbox 消息、更新 TeamTask 或触发调度。

### C-H05 — Leader 能力门

Leader Agent 必须同时具备 Persistent Resume 和历史 Replay/Read 能力。只具备 Resume 的 Agent 可以成为 Teammate，但不能承担可恢复正文的 Leader。

## 协作、运行时与安全

### C-A01 — AppService 权威

持久 Team 状态转换全部进入 AppService。Tauri、Engine、Go CLI、MCP 和前端服务只做适配。

### C-A02 — SQLite 与 TaskRuntime 分工

SQLite 是 TeamRun/TeamTask/mailbox 和恢复绑定的事实来源。TaskRuntime 只投影活动执行、进度、取消、去重和有限终态。

### C-A03 — Durable wake-up

需要异步协调的提交通过 transaction + domain-event outbox 唤醒常驻协调器。重复投递、轮询和应用重启不造成重复执行。

### C-A04 — 长任务响应性

draft、restore 和 execute 快速返回 task snapshot，在后台继续；只禁用冲突操作，导航和无关 CRUD 保持可用。

### C-T01 — Team tools

Leader 和 Teammate 通过任务板与 mailbox 协作。MCP 或 CLI fallback 最终调用同一个 AppService workflow。

### C-T02 — 成员最小权限

工具凭据绑定 tenant、Team、TeamRun 和 TeamMember。Teammate 只能读取和修改自己拥有的任务；跨成员操作被拒绝。

### C-S01 — 诊断最小化

日志、task snapshot 和公开错误只包含身份、阶段、时长和安全错误码；不包含 prompt、生成正文、历史回放、原始工具输入、凭据、环境变量值或 resume token。

### C-S02 — 本地确定性测试

自动测试使用 fake ACP/Native 和临时数据库。真实 Provider 与网络只用于最终可选 smoke。

