# Team 聊天工作台：不可变执行契约

本文件只记录 Issue #21 跨 Ticket 稳定的决策。具体 DTO、函数和组件名称由当前卡在既有接缝中确定。

## 领域与 Authority

### TW-D01 — Team 与 Conversation 隔离

Team 聊天、流式事件、历史回放、任务投影和恢复对 Conversation Session、Turn、Part、Question、QuestionTurn 与 Content Node 保持零写入。

### TW-D02 — 正文归 Provider

用户消息、Agent 正文、思考正文和工具正文由成员 Provider Session 持有。Team、Conversation、operation log、durable task snapshot 和 settings 不建立正文副本。

### TW-D03 — 结构化事实归 Team

Team、成员、TeamRun、TeamTask、审核结果、mailbox、执行上下文键和任务状态继续由 SQLite 持久化。时间线中的任务卡从这些事实重建，不属于聊天 transcript。

### TW-D04 — Provider 绑定归 Agent Execution

真实 Provider Session ID、Resume Anchor、workspace 和 Provider 元数据由 Agent Execution 持有。Team 只保存稳定 `execution_context_key`。

## 聊天工作台

### TW-U01 — Chat-first

Team 主页面采用聊天应用信息架构：Team 导航、单一活动成员时间线、成员导航和固定 composer。Team 管理表单退居次级操作。

### TW-U02 — 独立成员时间线

每个 TeamMember 对应一个独立 Provider Session。页面始终只显示一个活动成员时间线，不构造混合群聊 transcript。

### TW-U03 — Leader 默认与头像切换

首次进入 Team 默认选中 Leader并标记群主。头像切换只改变可见 Session 和 composer 接收者，不取消执行、不更改任务 owner、不创建新 Session。

### TW-U04 — 当前成员直聊

普通 composer 向当前活动成员发送消息；Leader 与 Teammate 均可直聊。团队任务模式只在 Leader Session 提供。

### TW-U05 — 后台持续

切换成员或离开页面不终止活动 turn。非活动成员通过头像状态、未读或完成提示暴露进展。

### TW-U06 — 单时间线滚动

只有用户接近底部时自动跟随增量；阅读旧内容时新事件不得强制跳到底部。恢复和切换保持布局稳定。

## Session Event

### TW-E01 — 单一通用事件契约

ACP、Antigravity 和未来 Provider 把原生事件翻译为同一 Session Event；Team、transport 和前端不按 Vendor、Agent ID 或协议分支。

### TW-E02 — 稳定身份与顺序

事件包含足以隔离 Team、member、execution、turn/item、event、sequence 的稳定身份。delta 更新原 item；重连、轮询和重复投递不追加副本。

### TW-E03 — 事件家族

契约覆盖用户消息确认、assistant text delta/snapshot、processing/thinking、tool start/update/result、task projection/status/result、notice、terminal result、cancel 和 error。

### TW-E04 — Replay 与 Live 分离

回放事件显式标记为 replay，仅构造界面投影；不会执行工具、写 mailbox、改变 TeamTask 或触发调度。Live 事件才能沿授权工作流改变结构化事实。

### TW-E05 — Provider 事实保真

只展示 Provider 实际提供的事实。缺少思考正文时显示 processing 或已声明的有限元数据，不合成隐藏推理。

### TW-E06 — 临时事件缓存

活动 Session 投影只保存在有界进程内缓存，用于页面返回和订阅重连；应用退出即清空，不形成持久 transcript。

## 任务交互

### TW-P01 — 单 composer 双模式

Leader composer 在普通聊天与团队任务模式间显式切换。进入任务模式本身不创建 TeamRun，提交后才启动后台 draft。

### TW-P02 — 内联审核卡

Leader 草稿以 TeamRun/TeamTask 投影的内联计划卡展示。编辑、推荐 owner、排序和确认都在卡内完成；AppService 校验仍是 Authority。

### TW-P03 — 人工门不变

确认成功前 Teammate 执行次数为零。确认原子冻结 owner 与顺序；失败、断线和直接聊天都不造成自动移交或 fallback。

### TW-P04 — Teammate 任务投影

确认后的 TeamTask 投影到其 owner 的时间线，并在重启后从 TeamTask 事实重建。执行文本和工具活动仍来自 Provider Session。

### TW-P05 — Leader 聚合与跳转

Leader 计划卡聚合所有子任务状态；点击任务切换到 owner 并定位任务锚点，不修改任何任务事实。

## 恢复

### TW-R01 — 结构化现场优先

打开 Team 先返回并呈现 SQLite 结构化现场。历史正文允许渐进出现，不要求瞬时完整。

### TW-R02 — 活动成员优先

活动成员优先 replay；非活动成员以有界并发后台恢复。切换成员调整优先级，不丢弃已完成投影。

### TW-R03 — 明确恢复状态

成员至少区分 not-started、restoring、ready、partial、unavailable。Resume 可用但历史不完整时允许继续直聊，并明确 partial。

### TW-R04 — Replay/Live 合并

回放期间到达的 live 事件必须按稳定序列缓冲或合并；回放完成不能覆盖、乱序或复制较新的 live item。

### TW-R05 — 锚点失效显式

Resume Anchor 无效时返回 unavailable。Runtime 不在同一 context key 下静默新建 Session 并伪装恢复成功。

### TW-R06 — 故障隔离

单个成员 replay 或 resume 失败不清除 TeamRun/TeamTask，也不阻塞健康成员恢复。

## 能力与 Session Adapter

### TW-A01 — 语义能力准入

成员准入依据 Persistent Resume、至少 user/assistant text History Replay、Live Events，不依据 ACP/Native 协议名称。前后端采用同一能力事实。

### TW-A02 — 富事件能力独立声明

基础准入与 thought/tool 历史保真度分开声明。UI 对缺失富事件显示能力限制，不把缺失数据当作空的完整历史。

### TW-A03 — Adapter 所有权

Session Adapter 拥有 Provider 进程、Resume Anchor、历史读取和 native-to-generic event translation。Team 只调用语义接口。

### TW-A04 — ACP 路径

ACP Adapter 把现有 AgentText、AgentThought、ToolActivity 和 Provider replay 转为通用事件，同时保留现有 final-text 兼容行为与权限策略。

### TW-A05 — Antigravity 真实锚点

Antigravity 每轮启动一个 `agy` 进程；从非空 init/result 捕获真实 Conversation ID，后续通过该 ID resume。合成 Native Session ID 不能成为 Provider Anchor。

### TW-A06 — Antigravity Provider 历史

Antigravity History Replay 读取 Provider 自有 conversation store，优先完整 transcript，降级简化 transcript。缺失或损坏返回 partial/unavailable。

### TW-A07 — Parser 层级

Team 不调用 Conversation Adapter。共享格式逻辑只能位于 Team 与 Conversation 之下；跨语言无法共享实现时共享 fixtures 和行为契约，而不是建立上层领域依赖。

### TW-A08 — OneShot 回归锁

Persistent Session Event 扩展不改变现有 OneShot 的创建、错误、超时、cleanup、workspace 删除和日志行为。

## 应用边界、响应性与隐私

### TW-B01 — AppService Authority

持久 Team 状态变化继续进入 AppService。Tauri、Engine、Go CLI、MCP 和前端 service 只适配。

### TW-B02 — 后台工作

member turn、draft、replay 和 restore 快速返回 task/stream snapshot 后后台运行，不持有全局 app lock；只禁用冲突操作。

### TW-B03 — Transport 完整

状态变化与可编程状态读取通过 Engine 暴露并生成 contract；Go CLI 只调用 Engine。Tauri event 以订阅为主，snapshot/polling 为恢复路径。

### TW-B04 — 前端 Service 边界

页面、hook、schema、reducer 和组件只调用 `frontend/src/services`，不直接 `invoke` 或监听 Tauri event。

### TW-B05 — 诊断最小化

日志、公开错误和 durable task snapshot 只包含身份、阶段、时长和安全错误码，不包含 prompt、正文、tool payload、credential 或 Resume Anchor。

### TW-B06 — 视觉与可访问性

使用 foundation/common 组件和语义主题 token；成员切换、composer、审核和任务跳转支持键盘与可见焦点；窄屏保持当前成员与接收者可见。
