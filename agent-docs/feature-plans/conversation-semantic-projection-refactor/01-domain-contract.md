# Conversation 语义分组与内容投影：领域合同

## 1. 术语

### Session

一次来源会话。负责会话边界、来源身份、tenant 归属和会话级元数据。

### Turn

来源系统中的物理交互轮次。Turn 是事实，不因为 Question 合并或拆分而复制、拼接或改写。

### Part

Turn 内的原子来源内容或执行事实。Part 保存原始内容、来源身份、类型和顺序。对于命令执行，
Part 的边界应尽可能与来源系统的一次真实 execution 一致。

### Question

完整用户问题的稳定语义分组身份。它可以聚合因中断、“继续”跟进或微调追问产生的多个物理
Turn。Question 不是正文快照，也不是 Card 集合。

### QuestionTurn

`conversation_question_turns` 中的 membership。它回答“哪个 Turn 属于哪个 Question、
在 Question 中处于什么顺序、为何形成这个归属、关系何时建立或变化”。

### Content Node

从一个源 Part 投影出的展示节点。它用于表达 prompt、answer、code、command、result 等结构化
展示单元。Content Node 可以是一对一或一对多投影；它不是新的持久化源事实。

### Card

前端对 Content Node 的一种视觉组件。Card 没有独立数据库身份，后端领域和公开契约不应使用
Card 代指 Part、Question 或 Content Node。

## 2. Authority 分配

| 信息 | Authority |
|---|---|
| 会话边界与来源会话身份 | `conversation_sessions` |
| 物理轮次及其顺序 | `conversation_turns` |
| 原始内容/执行事实及其顺序 | `conversation_parts` |
| 完整用户问题的稳定身份 | `conversation_questions` |
| Question 与 Turn 的归属、顺序、分配来源和关系时间 | `conversation_question_turns` |
| 搜索内容 | 可重建的独立索引 |
| 展示节点 | 由 Question–Turn–Part 投影得到的 Content Node |
| 前端卡片 | UI 组件状态，不是领域持久化状态 |

同一个含义只能有一个 Authority。兼容 DTO 可以临时复制形状，但不能重新获得写入权威。

## 3. `conversation_questions` 最终边界

该表可以保留：

- Question 稳定 ID；
- tenant 与 session 归属；
- 已有真实消费者需要的轻量标题或时间元数据；
- 未来经过独立规格证明必须由 Question 自身拥有的元数据。

该表不得保留：

- `question_text`；
- `answer_text`；
- `code_text`；
- `command_text`；
- `question_index`；
- `grouping_origin`。

前四项属于 Turn–Part 事实的投影；`question_index` 可由 membership 与 Turn 顺序推导；
`grouping_origin` 描述的是具体 Turn 的分配关系，应位于 `conversation_question_turns`。
不得为了搜索方便把正文重新塞回 Question 表；搜索使用可重建的独立索引。

## 4. `conversation_question_turns` 最终职责

在保留现有主外键和租户模型的基础上，membership 至少要表达：

- `question_id`；
- `turn_id`；
- Question 内确定性 `turn_order`；
- 自动 reconciliation、人工合并/拆分等 `assignment_origin`；
- 首次建立关系的时间；
- 最近关系变化时间。

字段名可按现有仓库命名约定调整，但语义不得丢失。必须满足：

1. 一个 Turn 在同一有效视图中至多属于一个 Question；
2. Question 与 Turn 属于同一 tenant 和 session；
3. 顺序稳定且不依赖返回数组的偶然顺序；
4. 人工归组形成 reconciliation fence，不被后续自动同步静默覆盖；
5. 关系历史迁移和重跑具备幂等性。

## 5. Question Detail 投影

标准读取形状是：

```text
Question metadata
  + ordered QuestionTurn memberships
    + Turn facts
      + ordered Part facts
        + projected Content Nodes
```

投影必须保持两个身份层级：

- 源身份：Session、Turn、Part，用于事实追溯、raw export、重新同步和 evidence；
- 展示身份：Question 与 Content Node locator，用于层级浏览、搜索命中和精确深链。

一个 Content Node locator 至少可解析到 Question、Turn、源 Part 和 Part 内稳定片段。它不能只用
数组下标。对单节点 Part，前端短 ID 来自 Part ID 的稳定短形式；对一对多 Part，短 ID 使用
“Part 短 ID + 稳定片段后缀”。Question prompt 身份来自 Question 和对应用户 Turn，不来自 Card。

## 6. Shell Execution 不变量

- 来源中的一次真实 Shell Execution 对应一个权威命令 Part；
- 原始命令、来源 execution ID、顺序与结果关联无损保存；
- 多条展示命令通过一对多 Content Node 投影产生；
- 纯分隔 `printf` 不产生命令节点，但其描述可成为相邻节点的 `command_label`；
- 执行结果与原始 execution/Part 关联，不按展示节点复制；
- 只有在来源事实可靠时，历史拆分记录才允许重同步为新模型。

## 7. Merge / Split 不变量

- merge：选择保留的 Question 身份，迁移 QuestionTurn membership，并重映射索引、evidence 和深链；
- split：选择 Turn 子集创建新 Question，迁移 membership，并按事实 locator 重映射消费者引用；
- 两者都不复制或拼接 Turn/Part 正文；
- 重复执行同一操作不产生额外 Question、membership 或内容；
- 无法唯一重映射的引用进入显式审计状态，不做静默猜测。
