# Conversation 语义分组与内容投影重构：执行总览

| 字段 | 值 |
|---|---|
| 状态 | Ready for Agent |
| 日期 | 2026-08-25 |
| 父 Issue | [#3](https://github.com/util6/assetiweave/issues/3) |
| 执行模型 | Luna |
| 计划范围 | [#4–#16](https://github.com/util6/assetiweave/issues/3) |

## 1. 文档职责

GitHub Issue 是计划工作与验收标准的权威来源。本目录不复制每张工单的完整规格，
只记录所有工单共同依赖的领域裁决、迁移边界、验证矩阵和 Luna 执行协议。

出现冲突时按以下优先级处理：

1. 仓库根 `AGENTS.md` 与已生效的架构约束；
2. 当前执行子 Issue 的 `What to build`、`Acceptance criteria` 与原生 blocker；
3. 本目录的领域合同、迁移规则和验证矩阵；
4. 既有代码和测试所反映的当前实现事实；
5. 父 Issue #3 的早期文字。

父 Issue #3 形成后需求有一次重要修订：`conversation_questions` 不删除，而是瘦身为
稳定 Question 身份与轻量元数据容器。子 Issue #4–#16 和本目录以该修订为准。

## 2. 目标状态

Conversation 的权威事实模型为：

```text
Session
  └─ Turn
      └─ Part

Question
  └─ conversation_question_turns
      └─ Turn
```

读取侧在事实之上建立投影：

```text
Session → Question → Turn → Content Node
```

核心结果：

- `Session`、`Turn`、`Part` 保存来源事实；
- `Question` 表达完整用户问题这一语义分组身份；
- `conversation_question_turns` 是 Question–Turn membership 的唯一 Authority；
- Question 合并、拆分和重新归组只改变 membership 与引用映射，不复制正文；
- Card 只是前端展示组件，不是持久化实体；
- 一个原始 Part 可以投影零个、一个或多个 Content Node；
- 一次 Shell Execution 在存储侧保持一个原始执行单元，在展示侧仍可逐条显示命令。

## 3. 明确排除

本轮不做以下事情：

- 删除 `conversation_questions`；
- 新建第二张与 `conversation_question_turns` 竞争的 membership 表；
- 在 Question 表保存问题、回答、代码或命令正文；
- 在 Question 表保存可推导的顺序或 Turn 分配来源；
- 为未来设想预加未被真实消费者使用的字段；
- 通过拼接历史拆分命令伪造不存在的原始来源事实；
- 以 UI 数组索引、Card 短 ID 或平行数组作为跨层身份；
- 在依赖尚未完成时提前删除兼容读取路径。

## 4. 工单阶段

| 阶段 | Issue | 交付重点 |
|---|---|---|
| 基础 seam | #4、#5 | membership Authority 与 Content Node 契约 |
| 分组和 adapter | #6–#8 | reconciliation、Codex 原始执行、adapter 一致性 |
| 消费者迁移 | #9–#12 | Search、Memory、Export、前端层级读取 |
| 数据收口 | #13–#15 | 审计修复、Question 瘦身、旧 Card 契约删除 |
| 最终验收 | #16 | 全链路、性能和文档证据 |

原生 blocker 是开工顺序的 Authority。表格只用于导航，不取代 blocker。

## 5. 完成定义

整个父 Issue 只有在以下事实同时成立时才完成：

1. Question 内容完全可由 membership 和 Turn–Part 事实投影；
2. `conversation_questions` 不含正文、可推导顺序和分配来源字段；
3. 新写入不再拆分存储一次 Shell Execution；
4. 前端保持逐条命令展示且使用稳定 locator；
5. Search、Memory、Export、Engine、CLI 和前端均已迁移；
6. 历史数据有 dry-run、备份、修复、验证和回滚证据；
7. 旧 Card DTO 与平行数组兼容契约已在消费者切换后删除；
8. 完整质量门禁和性能基线通过。
