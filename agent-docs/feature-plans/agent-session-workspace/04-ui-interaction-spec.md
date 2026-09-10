# UI 与交互规范：AionUi 等价聊天工作区

> 本文定义用户可见行为。实现对照 AionUi 固定 commit，但所有组件、样式、状态与服务必须落入 AssetIWeave 体系。

## 1. 视觉目标

### U-001：内容优先

聊天正文与执行步骤占据主要空间。正常工作区不使用营销式 hero、大号描述或卡片套卡片。

### U-002：平面高密度

Header、tabs、timeline、composer 通过一像素语义边界组织。圆角用于输入 surface、按钮、用户消息和结构化内联卡；不把每条 assistant 内容包成大圆角 Panel。

### U-003：容器驱动宽度

聊天内容宽度由实际 lane/container 决定，不由“Team/Single”模式硬编码：

- container < 720px：timeline/composer 使用可用宽度；
- container ≥ 720px：保留舒适侧 gutter；
- Team parallel lane 约 400px 时不额外挤压正文；
- Team single 和 standalone interactive 使用同一 gutter 规则。

### U-004：主题原生

颜色、边框、阴影、状态与文字全部使用 semantic Theme Tokens。视觉相似来自结构、密度和层次，不通过复制 AionUi raw token 达成。

## 2. AgentSessionWorkspace 解剖

```text
┌──────────────── Session Header ────────────────┐
│ identity · model · status · mode · actions     │
├──────────────── optional status/plan slot ─────┤
│                                                │
│                Session Timeline                │
│  User Request                                  │
│  Assistant / Thinking / View Steps / Result    │
│                                                │
│                             New activity ↓     │
├──────────────── Composer (interactive only) ───┤
│ attachments/status/input/actions/send-or-stop  │
└────────────────────────────────────────────────┘
```

- Workspace 根节点必须 `min-height: 0` 并填满父容器。
- Header 和 Composer 不跟随 timeline 滚动。
- Timeline 是唯一纵向主滚动容器。
- Observer 不保留空 composer 占位高度。

## 3. Header

### 必显信息

- Agent display name；缺失时 Agent ID；
- 模型名称；缺失时不显示空 pill；
- running/restoring/succeeded/failed/cancelled/unavailable 状态；
- observer 模式的“只读执行现场”；
- Team lane 中的 Leader/Teammate 身份；
- Memory observer 中的 scope 与返回 Task 操作。

### 行为

- 名称溢出使用单行截断，完整值通过 tooltip/accessible name 获取。
- 状态不只依赖颜色，同时有图标或文本。
- 运行状态改变不移动主要操作顺序。
- Header action 使用 capability 决定显隐。
- unavailable 时 Header 仍显示已知 identity/context。

## 4. Timeline 与 Turn

### 4.1 Turn 容器

- Turn 是语义组织，不是必须有可见大边框的卡片。
- 每个 Turn 可以包含：User Request、Assistant、Thinking、Processing、多个 StepGroup、Task/Plan、Notice、Terminal/Error。
- Turn 内 item 按公开 sequence 渲染。
- 每个可定位 item 有稳定 DOM anchor；流式更新不得 remount 已存在 item。

### 4.2 User Request

- 右侧或视觉上明确区别于 assistant；使用克制的 filled surface。
- 保留换行；支持文本选择与复制。
- Memory request 显示“Memory Agent 输入”来源标签，避免误认为用户手动发送。
- 超长 request 使用 CollapsibleContent；截断状态始终可见。

### 4.3 Assistant Text

- 使用开放式正文布局，避免整段套大 Card。
- Markdown 支持段落、列表、代码、表格、引用、链接和 inline code。
- Provider 内容按受控 Markdown 渲染；HTML/script 不执行。
- 流式 delta 在同一 item 中增长；光标/动画遵守 reduced motion。
- hover/focus 时显示复制与时间；窄屏通过可触达操作菜单提供。

### 4.4 Thinking

- Header 包含 thinking/processing 图标、状态文字、展开按钮。
- live running thinking 默认展开；completed replay 默认折叠。
- 用户手动切换后，后续 delta 不改变其选择。
- 内容区使用比正文更弱的层级，但保持可读对比度。
- 只有 processing 时显示状态行，不显示空白展开区域。
- Thinking 失败不替代 assistant 或整个 Turn。

### 4.5 Notices 与 Error

- Notice 为低强调内联状态；Error 为语义错误 surface。
- 错误显示 public code/message/retryability；不展示原始 stderr。
- Retry 按钮只在外部 capability 提供时出现；Memory observer 内不出现。

## 5. `查看步骤 · N`

### 5.1 分组算法

1. 遍历一个 Turn 的有序 items；
2. 遇到 Tool Step 时开始/延续当前 StepGroup；
3. 遇到 assistant、user、thinking、task、plan、notice、terminal/error 时关闭当前 StepGroup；
4. 后续 Tool Step 创建新组；
5. `N` 是逻辑 Tool Step 数，不是 start/update/result 事件数。

### 5.2 Group Header

必须包含：

- checklist/activity 图标；
- 本地化标题 `查看步骤`；
- `· N`；
- 总体状态；
- 展开箭头；
- 键盘可操作 button 语义与 `aria-expanded`。

总体状态优先级：failed > running > cancelled > succeeded > pending。

### 5.3 默认展开

| 场景 | 默认 |
|---|---|
| 新 live group，含 running/pending | 展开 |
| live group 刚完成 | 保持当前状态 |
| completed replay group | 折叠 |
| failed group | 展开 |
| 用户已手动选择 | 保持用户选择 |

### 5.4 Step Row

每行显示：

- 状态图标/点；
- tool name，缺失回退为本地化“工具调用”；
- title/summary；
- running indicator；
- 详情 chevron，仅在有详情时显示。

重复 name 通过 title/summary/command/path 区分。状态变化不改变 row identity。

### 5.5 Step Detail

按存在性和固定顺序展示：

1. Input
2. Output
3. Diff
4. Location
5. Error
6. Exit

规则：

- JSON 使用等宽、两空格缩进和稳定 key 顺序；
- command 单独显示命令与 cwd；
- terminal 分 stdout/stderr，显示 exit code/signal；
- preformatted 内容可选择、复制，内部溢出不撑宽 lane；
- Diff 使用现有 Diff renderer；不存在 renderer 时显示统一 diff text，不退化为 `[object Object]`；
- image/artifact 只使用现有安全查看能力；
- truncated 显示 original/retained size 和明确标签；
- unknown block 显示 Provider type 与 bounded text。

## 6. Terminal 与 Plan/Task

### Terminal

- terminal 表示 Agent execution 终态，不替代最后 assistant 正文。
- 成功使用低强调状态；失败/取消使用明显但局部状态。
- terminal text 与 assistant 正文相同则不重复显示。

### Plan/Task

- Team Plan 和 TeamTask 保持 typed card/slot。
- Plan 位于 Leader 的真实时间顺序。
- TeamTask 位于 owning member lane，支持稳定 anchor。
- Memory validate/publish 不伪装为 Agent tool；它们留在 Task View。

## 7. Composer

### 7.1 结构

```text
optional quote/attachments/status
multiline input
capability actions                         send/stop
```

### 7.2 键盘

- Enter：发送；
- Shift+Enter：换行；
- IME composing：Enter 不发送；
- Escape：关闭当前 mention/slash/attachment overlay；
- 发送后焦点留在当前 composer，除非操作进入需要确认的 modal。

### 7.3 运行态

| capability/state | 主按钮 |
|---|---|
| idle + valid input | Send |
| running + stop | Stop |
| running + interrupt + valid input | Interrupt/Send，按 Team 现有语义 |
| running + queue + valid input | Queue |
| 无合法操作 | Disabled，并显示真实状态原因 |

- Busy 只禁用冲突动作。
- 乐观 user item 使用 client identity；server ack 后与真实 item 合并，不重复。
- 发送失败保留 draft/失败消息与 retryable 状态。

### 7.4 可选能力

附件、@file、@session、slash command、model selection、permission response 和语音只在真实 capability + service 都存在时显示。本规格提供 slot/contract，不伪造未实现功能。

### 7.5 Observer

Observer 不渲染 composer、输入热键、permission action 或占位 footer。Task cancel/retry 在 Task Center Header。

## 8. Team 工作区

### 8.1 页面结构

```text
Team navigation rail | Team header + member tabs
                     | parallel lanes / single lane
```

- populated workspace 移除大 PageHeader；
- Team navigation 使用高密度 row；
- Header 放 Team 名称、view toggle、details/edit/create；
- Member tabs 紧邻 lanes。

### 8.2 Member Tabs

- Leader 第一；Teammate 按 `sort_order`；
- 显示成员/Agent identity、状态、未读；
- `aria-selected` 和可见焦点；
- ArrowLeft/ArrowRight/Home/End 导航；
- 单列模式点击切换 active lane；并行模式点击滚动到 lane。

### 8.3 Parallel

- 每 lane `flex: 1 1 400px`；
- 两成员可根据容器允许降至 240px，但正文仍须可用；三成员及以上保持 400px floor；
- 超出横向滚动，使用 proximity snap；
- adjacent lanes 使用语义 separator；
- 每 lane 独立 timeline/composer；
- inactive lane 更新不抢焦点和横向位置。

### 8.4 Single

- active lane 全宽；
- 从 parallel 进入时，以触发 lane 为 active；
- 返回 parallel 保留 active member 与各 lane scroll；
- view mode 按 Team ID 保存 UI preference；窄屏临时 single 不覆盖 preference。

### 8.5 Team 状态

必须覆盖：empty roster、warming/restoring、member ready、running、queued、paused、runtime failed、session stopped、TeamRun terminal。状态不以全屏 spinner 覆盖可读历史。

## 9. Memory Observer

### 9.1 入口

- Task Center Memory detail 的 Agent Stage 显示“查看执行现场”；
- 仅当 `agentSessionRef` 存在时可操作；
- running 与 terminal Stage 均可进入；
- 点击后在 Task Center 详情区域的子视图或同 route 可返回视图中打开，不使用阻断式 modal。

### 9.2 Header

显示：

- `Session / Project / Global / Recall Memory` scope；
- Agent/Model；
- running/terminal/unavailable；
- “只读执行现场”；
- 返回任务详情。

### 9.3 内容

- 第一条可见 request 是本次实际 Memory Agent prompt；
- 后续使用共享 timeline；
- Agent terminal 后可以显示“返回任务详情查看校验/发布结果”的状态提示；
- observer 关闭不取消任务；
- Task 被取消不自动销毁已投影 items。

### 9.4 Unavailable

任务尚未进入 Agent 时不提供入口；已有 ref 返回 `notFoundOrExpired` 时统一显示“当前进程没有可用执行现场”（覆盖已淘汰、应用重启和未知 ref，避免泄漏存在性）；网络/transport 加载失败显示可重试加载错误。页面始终保留返回 Task 操作，不显示空白 timeline。

## 10. Responsive Matrix

| Width | Chat | Team | Steps | Composer |
|---|---|---|---|---|
| 320 | full width，无侧 gutter | single only | detail 内部滚动 | 操作收进紧凑布局 |
| 768 | full width/小 gutter | temporary single | 正常展开 | 保持多行输入可用 |
| 1024 | container gutter | parallel 默认 | lane 内不撑宽 | 每 lane 独立 |
| 1440 | bounded readable content | parallel，多 lane 横滚 | 完整 detail | 完整 action row |

布局应按 container query/ResizeObserver 等现有模式基于实际容器，而不是只看 viewport。

## 11. Accessibility

- Workspace 有可辨识 heading/label；
- Timeline 使用 `role=log` 或等价语义，`aria-live` 不逐 token 播报；
- StepGroup/Thinking 使用原生 button；
- Member tabs 使用 tablist/tab 或同等键盘语义；
- 状态同时有文字/图标；
- 错误使用恰当 alert，普通进度使用 polite status；
- focus 不因流式更新丢失；
- reduced-motion 下关闭 shimmer/typing/scroll animation；
- 正文正常文字对比度达到 4.5:1；大文字/图标达到 3:1。

## 12. 国际化

所有用户文案进入既有 i18n resources。至少覆盖：

- 查看步骤、输入、输出、差异、位置、错误、退出；
- 处理中、恢复中、已完成、失败、已取消、不可用；
- 只读执行现场、返回任务、查看执行现场；
- 历史详情未提供、内容已截断、回到最新、新活动；
- Tool/Agent/Model 缺失回退文案。

中文和英文测试至少各验证关键 label 不缺失；不得在组件中散落默认英文作为长期文案。

## 13. 视觉验收场景

每个场景与 AionUi 固定基准并排对照：

1. 空 Session；
2. 用户一问一答；
3. streaming assistant；
4. streaming thinking；
5. processing-only；
6. 两个连续 Tool Steps；
7. running StepGroup；
8. failed Step；
9. command stdout/stderr；
10. Diff；
11. long/truncated content；
12. Team 两成员 parallel；
13. Team 三成员横向 overflow；
14. Team single；
15. Memory running observer；
16. Memory terminal observer；
17. unavailable observer；
18. 320/768/1024/1440；
19. dark/light；
20. reduced-motion。

通过标准是层次、密度、交互和状态等价；不要求像素复制 AionUi 的颜色或导入其 CSS。
