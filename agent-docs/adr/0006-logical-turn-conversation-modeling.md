# 0006: 会话三层架构归一化与问题语义聚合建模（Question-Turn-Part-Card）

> **重要等级**：核心级（P1）  
> **状态**：已接受  
> **决策日期**：2026-06-04  
> **决策证据**：`019e92fa`  
> **记录日期**：2026-08-25  

## 背景

在启动对话记录管理板块时，行业内大多数工具（如 Chat 导出插件）均直接将整场会话作为一个不可分割的 Session 整体（大黑盒/JSON Dump）进行存储与检索。在 AI 编程场景下，整场 Session 通常包含数十次不同主题的提问，若以 Session 为粒度进行搜索或喂给上层 AI，会导致严重的 Context Window 浪费与检索失焦。

然而，如果机械地按第三方工具的原始物理交互流存储，又会遇到意图碎片化问题：单次用户需求常因长度截断、报错重试、用户输入“继续/好的”而被物理打碎为多次物理 Turn。

系统需要一套既能**保真还原第三方原始事实**，又能**支持高效语义检索与清晰 UI 呈现**的会话领域模型。

## 决策

确立由**输入契约层、持久化领域层、展示投影层**构成的三层会话架构体系：

```text
① 输入契约层 (Adapter):     Session ──> Turn ──> Part (content_card?)
                                          │
② 持久化领域层 (Persistence): Session ──> Turn ──> Part (content_card_json)
                                ▲         │
                                │   (1:N) │ (QuestionTurn)
                                └────── Question (物化搜索与聚合单元)
                                          │
③ 展示投影层 (Projection):   SessionDetail ──> QuestionDetail ──> Card (Projection of Part)
```

1. **输入契约层（Adapter Contract）**：标准化表达第三方来源事实，仅包含 `NormalizedConversationSession` → `NormalizedConversationTurn` → `NormalizedConversationPart`，不侵入 Question 抽象。
2. **持久化与事实骨架（Domain Fact Skeleton）**：
   - **`Session`**：第三方 Agent 一次完整会话的资产边界。
   - **`Turn`**：来源系统中的一次原始用户交互轮次，是增量同步的基础单元。
   - **`Part`**：Turn 内不可再分的标准化内容事实块，承载 `role`、`kind`、`command`、`text` 与 `content_card_json`，是数据库最小持久化实体。
3. **语义问题聚合（Semantic Question Grouping）**：
   - 引入 **`Question`（`conversation_questions`）**，通过 **`QuestionTurn`（`conversation_question_turns`）** 关联 1:N 个物理 `Turn`。
   - 自动识别“继续”、“确认”、“好的”等跟进词（`AutoMerged`），将碎片化的物理交互聚合成一个自包含的逻辑意图。
   - 在 Question 上物化 `question_text`, `answer_text`, `code_text`, `command_text`，作为全文搜索与 Memory 的最小消费单元。
4. **展示投影与卡片（Card as Projection）**：
   - 数据库不设立独立的 `conversation_cards` 表；**`Card`（`ConversationCard`）是 `Part` 面向消费侧的只读投影（View Model）**，满足 `card_id == part_id`。
   - 读取端通过 `ConversationQuestionDetail` 提供结构化下钻数据。

## 备选方案

### 全量 Session 黑盒存储（Session-level Blackbox Dump）

- 缺点：检索粗糙，无法精准定位具体代码变更或命令输出；AI 回忆时极易耗尽 Token 上下文。
- 结论：否决。

### 纯物理五级嵌套表实体（把 Card 和 Question 全做成物理实体）

- 缺点：层级过多导致数据库写入与迁移极度僵化；且第三方 Adapter 并不产生 Question，强行在输入层定义 Question 会导致适配器协议与来源事实失配。
- 结论：否决。

## 后果

- **语义与事实清晰解耦**：`Turn` 和 `Part` 忠实记录来源系统的原始事实，`Question` 负责向上语义提炼与搜索加速，`Card` 负责向下解释与富文本呈现。
- 支撑了“App → 项目目录 → 会话 → 语义问题 → 内容卡片”的逐级下钻与短 ID 渐进式检索体验。
