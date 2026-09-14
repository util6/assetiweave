# Memory Q1–Q36：L2/L3 晋升、长期修订与 Context

## 1. 三层漏斗

```text
L1 generated observations
  -> application admission
  -> L2 project candidates
  -> Project Consolidation
  -> current L2 revisions
  -> application admission
  -> L3 candidates
  -> Global Consolidation
  -> current L3 revisions
```

层级变化只能由应用准入与成功 Consolidation 提交完成。Agent 输出的 nomination 不直接改变 `layer`。

## 2. L1 → L2 准入

### 2.1 可晋升内容

- 用户明确确认的项目决定；
- 用户明确声明的项目约束或不变量；
- 具有可复用价值的验证结论或失败原因；
- 持续阻塞、长期待办或研究结论；
- 被否决方案及其理由，且未来可能避免重复决策。

普通完成播报、短期进度、一次性命令结果、无证据建议和未归属项目内容不晋升。

### 2.2 立即候选

`project_decision` 或 `project_constraint` 可以在一个 generated Snapshot 后成为候选，但必须：

1. project key 不是 `unassigned`；
2. 至少一个 available reference 指向用户角色的 canonical Content Node；
3. source revision 与 Snapshot Work Order 一致；
4. nomination 的 statement 与引用归因由 Agent 给出，应用验证引用确实属于用户内容；
5. 不与当前 L2 的更晚有效 revision 重复或冲突。

应用验证身份、角色与范围，不通过关键字替代语义判断。

### 2.3 两次观察候选

`recurring_blocker`、`recurring_todo`、`research_conclusion` 需要两次连续有效观察：

- 来自两个 `publication_kind=generated` 的成功 Snapshot；
- 后一次 Snapshot sequence 紧随上一有新证据的 generated Snapshot；中间 reused Snapshot 不推进、不打断；
- project key 和 Item identity 相同；
- evidence fingerprint 有变化，且至少一个新 available reference；
- 在中间 generated Snapshot 中明确终态或缺席会中断 streak。

满足条件只进入候选队列，仍需重复、冲突与引用准入。

## 3. Project Consolidation

### 3.1 触发

仅在下列任一条件成立时为 project key 入队：

- 新 L2 candidate 通过应用准入；
- 当前 L2 source availability 变化；
- 后续证据提名 correction/supersession；
- contract、Skill 或预算策略版本变化且明确要求 scope rebuild。

普通 L1 水位、reused Snapshot、没有候选的进度变化不入队。

### 3.2 输入

- 当前有效 L2 revisions；
- 新候选及完整短引用映射；
- 候选相关的 L1 observation history；
- source availability；
- 已否决/被取代条目的轻量索引，用于避免复活旧结论。

### 3.3 输出与提交

Project Agent 返回结构化 operation：

```text
create(statement, rationale, category, sources)
revise(item_id, statement, rationale, sources)
supersede(old_item_id, replacement_statement, rationale, sources)
keep(item_id)
```

应用验证后在一个事务中写 L2 Item/revision/source/supersession，并更新 Project Consolidation last-success。失败不改变当前 L2。

同一 project key 串行；不同 project key 可并行。输入 fingerprint 相同则跳过 Agent。

## 4. L2 → L3 准入

### 4.1 明确全局规则

候选必须有 available 用户引用，并由 Agent 提名为 `global_rule`。应用验证引用角色、tenant 和 statement 的显式 scope metadata；没有明确全局/跨项目适用范围时保持 L2。

### 4.2 跨项目模式

`cross_project_pattern` 必须：

- 由至少两个不同 project key 的当前 L2 revision 独立支持；
- 每个项目至少一个 available reference；
- 不是同一 Session 被项目重映射产生的重复；
- statement 能以不包含项目私有进度的形式表达。

两个 worktree 被 Project Directory 合同判为不同项目时可以构成两个项目，但 Agent 必须说明其确为通用规律；应用不因路径数量自动推断语义。

## 5. Global Consolidation

- 只消费当前有效 L2 和合格 L3 candidates。
- 触发为每周维护水位或候选数量阈值达到。阈值属于版本化运行策略，首版冻结为 8 个待处理候选；手动维护重建可以绕过时间门但不绕过资格准入。
- 同一 tenant 只运行一个 Global Consolidation。
- 输入 fingerprint 不变时不调用 Agent。
- 输出 operation 与 Project Consolidation 同构，但 layer 固定 L3，禁止复制项目进展、项目待办和单次命令结果。
- 成功事务更新 L3 revision 与 Global last-success；Markdown 在事务后确定性重建。

## 6. 来源失效

### 6.1 L1

未晋升 L1 在下一 generated Snapshot 退出；引用标 unavailable。

### 6.2 L2/L3

已晋升条目继续 current，引用逐条更新：

- Session 删除：`unavailable/deleted`；
- Session missing：`unavailable/missing`；
- Session 或 source 排除：`unavailable/excluded`；
- source disabled：`unavailable/source_disabled`。

引用变为 unavailable 后：

- Recent/长期读取仍可显示条目及“来源不可用”；
- UI 不生成 Session 跳转；
- 该引用不能支持新的晋升、revision 或跨项目计数；
- Context 可继续使用已晋升 statement，但必须在内部 reference metadata 标明不可用；
- 所有引用不可用不会自动删除长期条目。

## 7. 纠正与 supersede

- 后续有效证据与当前条目冲突时，Agent 提名 `revise` 或 `supersede`。
- 同一逻辑事实的文本更新使用新 revision；适用对象发生本质变化或新结论取代旧结论时使用 Item supersession relation。
- 当前指针只在新 revision 完整验证并事务提交后切换。
- 旧 revision 和 source availability 历史只读保留。
- 普通读路径过滤 superseded/retired；审计与深度回忆可以在明确请求历史变化时读取链。

## 8. Context Resolver

### 8.1 输入

```text
project_path: optional
query: optional
token_budget: positive integer
```

project path 通过现有 Project Directory 解析合同规范化。

### 8.2 选择优先级

1. 当前有效 L3：全局规则、长期偏好、通用约束；
2. 当前项目的有效 L2；
3. 与 query/项目相关的少量成功 Session Memory；
4. L1 Snapshot 不默认整份注入，仅在调用者显式要求 recent scope 时选择相关条目。

每层先按 relevance，再按有效引用、最后修订时间和实际 usage 排序。usage 只影响选择，不改变事实内容或晋升资格。

### 8.3 输出

Context 返回：

- `text`：预算内确定性格式化正文；
- `revision`：所选 Item revision IDs、Session Memory revisions 与策略版本的 hash；
- `generated_at`：最大来源生成时间；
- `estimated_tokens` 与 `token_budget`；
- 结构化 references，包含 layer、item/revision、project、source availability；
- truncation/coverage 状态。

Resolver 只读 last-success，不等待 Consolidation，不调用 Agent，不解析 Markdown。

### 8.4 预算

- L3 与明确用户约束优先；
- 当前 L2 其次；
- Session detail 最后；
- 单项过长按语义段落边界缩短，并在 metadata 标记 truncated；
- token 估算复用现有实现；返回不得超过调用方预算；
- 所有层无可用内容时返回空 text 与稳定空 revision，而不是触发生成。

## 9. 深度回忆边界

现有 `assetiweave-memory` 和应用内深度回忆继续通过只读 API 检索当前 Memory 与 Conversation。它可以在解释历史变化时读取 supersede 链，但本规格不改变其多轮会话、工具或 UI。
