# T09：交付 Recall 单轮垂直切片

## Outcome

用户在「回忆」创建 Recall Session 并发送一次碎片线索；产品通过 `memory.recall` action 运行受限 ACP Agent，返回 `answer`、Session references、Content references 与 follow-up suggestions，前端可跳转现场。

## Blocked by

T08。需要稳定 hybrid tools 与 locator。

## Read

- Contracts：C-D05、C-D06、C-A01、C-A02、C-A05、C-R01–C-R08、C-S02、C-S03。
- Seams：S01、S06–S10、S13、S15、S16；Tests TS01、TS03、TS06。
- Gates：G0、G1、G2、G3、G7。

## Authority changed

新增 Recall workflow/binding、任务关联和结构化引用元数据的 SQLite authority，以及 Recall 专用 ACP 交互聚合；用户消息、Agent 回答和 turn 内容继续进入既有 Conversation 合同。

## Red test first

Fake ACP 收到用户线索后调用允许的 hybrid search/candidate read 工具并返回结构化结果。AppService 通过既有 Conversation 合同重读该 Recall turn；非法、跨 tenant、重复或不存在的引用被拒绝/剔除；前端 answer 不含内部 ID，引用组件可跳转。

## Execution steps

1. 定义 Recall workflow/turn execution 状态与结构化输出 schema。完成标准：answer 和三类数组独立校验，消息/回答进入 Conversation，内部 ID 只存在引用 DTO。
2. 注册 `memory.recall` action 与产品固定 Profile/Workflow/tool allowlist。完成标准：Agent/model 可配置，权限不由 Skill 决定。
3. 实现 Recall 专用 ACP aggregator。完成标准：允许工具活动与结构化完成事件，不复用 TranslationTextAggregator，OneShot 回归保持。
4. 建立 AppService create/send/read 的单轮路径。完成标准：长执行快速返回 task/session snapshot，结果可在数据库重开后读取。
5. 实现最小对话 UI 与引用组件。完成标准：loading/error/empty 明确，Session 与 content reference 复用现有导航。

## Acceptance

- [ ] Recall 由产品 Profile/Workflow/allowlist 定义。
- [ ] `memory.recall` 只决定 Agent/model。
- [ ] Agent 只能调用 T08 只读工具。
- [ ] 输出严格包含四个结构化字段。
- [ ] 非法/重复/跨 tenant locator 不进入可见引用。
- [ ] answer 不混入内部 ID，引用组件可跳转。
- [ ] Recall 消息与回答复用 Conversation，不存在 Memory transcript 副本。
- [ ] Translation OneShot 行为与测试保持不变。

## Non-goals

连续追问、取消/恢复、Agent 退出恢复、通用多 Agent Chat、完整 CLI。

## Ticket-specific stop

如果实现需要 Team 领域、通用聊天 transcript 或 Translation OneShot 文本聚合器，停止并报告。
