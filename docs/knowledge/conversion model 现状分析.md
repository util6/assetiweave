基于你当前 GitHub `main` 上的 Rust 模型、conversation SQL migrations 和 projection 代码来看，我建议你不要把这五个东西简单理解成：

```text
Session
  └─ Question
      └─ Turn
          └─ Part
              └─ Card
```

**UI 读取出来时很像这棵树，但底层真实模型不是这样。**

更准确的是：

```text
                         ┌──────── Question ────────┐
                         │                           │
Session ──────< Turn >────┴─ QuestionTurn          │
                 │                                  │
                 └──────< Part ──────> Card(Projection)
```

或者按基数展开：

```text
Session
 ├── 1 : N ── Turn
 │             └── 1 : N ── Part
 │                           └── 0 : 1 ── Card（投影，不是独立持久化实体）
 │
 └── 1 : N ── Question
               └── 1 : N ── QuestionTurn ──> Turn

其中：
一个 Turn 最多属于一个 Question
一个 Question 可以包含多个 Turn
```

这恰好体现了你现在这套 conversation 架构里三种不同层次：**原始对话结构、AssetIWeave 的语义分组结构、UI 展示结构。**

---

# 一、先给这五个概念定性

| 概念           | 本质                       | SQL 实体                   | Rust Domain Entity     | conversion/adaptor 输入               | UI DTO                       |
| ------------ | ------------------------ | ------------------------ | ---------------------- | ----------------------------------- | ---------------------------- |
| **Session**  | 一整个第三方会话                 | `conversation_sessions`  | `ConversationSession`  | `NormalizedConversationSession`     | `ConversationSessionDetail`  |
| **Turn**     | 一次用户交互轮次                 | `conversation_turns`     | `ConversationTurn`     | `NormalizedConversationTurn`        | QuestionDetail.turns         |
| **Part**     | Turn 内最小内容块              | `conversation_parts`     | `ConversationPart`     | `NormalizedConversationPart`        | QuestionDetail.parts         |
| **Question** | AssetIWeave 对 Turn 的语义分组 | `conversation_questions` | `ConversationQuestion` | **没有**                              | `ConversationQuestionDetail` |
| **Card**     | Part 的展示/语义投影            | **没有 Card 表**            | **不是持久化 Entity**       | `ConversationContentCardDescriptor` | `ConversationCard`           |

这里最重要的就是最后两行：

> **Question 不是 Adapter 原始数据结构的一部分。**
>
> **Card 也不是持久化数据实体。**

Adapter 真正输出的标准化结构只有：

```text
NormalizedConversationSession
    └── NormalizedConversationTurn[]
            └── NormalizedConversationPart[]
                    └── content_card?: ConversationContentCardDescriptor
```

代码就是这样定义的。

---

# 二、Session：会话边界

## 1. Conversion / Adapter 层

Adapter 给 AssetIWeave 的是：

```rust
NormalizedConversationSession {
    external_id,
    title,
    project_path,
    started_at,
    updated_at,
    source_locator,
    source_fingerprint,

    turns: Vec<NormalizedConversationTurn>,
}
```

所以在 conversion contract 看：

```text
Session
  └─ owns Turns
```

它是**第三方 Agent 一整个 session 的标准化表示**。

例如：

```text
Codex session abc123
Claude Code session def456
OpenCode session xyz789
```

经过各自 Adapter 后，都变成 `NormalizedConversationSession`。

---

## 2. 持久化实体

对应：

```rust
ConversationSession {
    id,
    source_id,
    adapter_id,
    external_id,
    title,
    project_path,
    started_at,
    updated_at,
    source_locator,
    source_fingerprint,
    missing,
    created_at,
    imported_at,
}
```

SQL 是：

```text
conversation_sessions
```

核心关系字段：

```text
id
source_id
adapter_id
external_id
```

并且：

```sql
UNIQUE(tenant_id, source_id, external_id)
```

也就是说：

> 同一个 Source 下，一个第三方 external session 只能对应一个 AssetIWeave Session。

SQL 当前已经加上 `tenant_id`，因此真正数据库 identity 是：

```text
(tenant_id, session_id)
```

而 Rust `ConversationSession` 本身没有 `tenant_id`，说明 tenant 是 repository/store context，而不是 conversation domain entity 自身的字段。

---

# 三、Turn：真实的交互轮次

Turn 是整个模型里非常重要的一层。

Adapter 标准结构：

```rust
NormalizedConversationTurn {
    external_id,
    turn_index,
    user_text,
    title,
    started_at,
    ended_at,

    parts: Vec<NormalizedConversationPart>,
}
```

持久化后则是：

```rust
ConversationTurn {
    id,
    session_id,
    external_id,
    turn_index,
    user_text,
    title,
    started_at,
    ended_at,
    fingerprint,
    missing,
    imported_at,
}
```

SQL：

```text
conversation_turns
```

关系：

```text
conversation_turns.session_id
                  ↓
conversation_sessions.id
```

因此逻辑上：

```text
Session 1 ────── N Turn
```

一个 Session 可以有很多 Turn。

---

# 四、Question：这里最容易误解

我认为这是你目前架构中最值得搞清楚的一层。

**Question ≠ Turn。**

而且：

> **Question 根本不是 Adapter / Conversion 标准化协议里的概念。**

Adapter 返回：

```text
Session
  Turn
    Part
```

没有：

```text
Question
```

Question 是 AssetIWeave **导入之后生成的二次语义结构**。

---

## 为什么需要 Question？

假设真实对话：

```text
Turn 1
User: 帮我分析这个架构
Assistant: ...

Turn 2
User: 好，继续
Assistant: ...

Turn 3
User: 那数据库应该怎么设计？
Assistant: ...
```

按照 Agent 原始 Session：

```text
Turn1
Turn2
Turn3
```

但从人的语义上：

```text
Question 1
    Turn1
    Turn2   ← “好，继续”不是一个新的真正问题

Question 2
    Turn3
```

这就是 Question 的存在意义。

你代码里的：

```rust
group_turn_ids_by_question()
```

明确做了这个逻辑。

例如：

```text
ok
okay
yes
continue
go ahead
确认
可以
好的
继续
继续吧
```

这种 acknowledgement 会被合并进前一个 Question。

合并以后：

```rust
grouping_origin = AutoMerged
```

正常单独形成的问题：

```rust
grouping_origin = Imported
```

此外 enum 里还预留：

```rust
Manual
```

。

所以从领域语义来说，我会这样定义：

> **Turn 是来源系统定义的交互单元。**
>
> **Question 是 AssetIWeave 定义的语义问题单元。**

这个区别非常合理。

---

# 五、Question 和 Turn 不是直接 FK，而有中间表

数据库并不是：

```text
turn.question_id
```

而是：

```text
conversation_question_turns
```

结构：

```sql
question_id
turn_id
turn_order

PRIMARY KEY (question_id, turn_id)
UNIQUE(turn_id)
```

tenant 化后是：

```sql
PRIMARY KEY (tenant_id, question_id, turn_id)
UNIQUE(tenant_id, turn_id)
```

因此逻辑关系实际上是：

```text
Question 1 ───── N QuestionTurn
                         │
                         └──── 1 Turn
```

而这个：

```sql
UNIQUE(turn_id)
```

非常关键。

它意味着一个 Turn：

```text
不能同时属于两个 Question
```

所以实际关系是：

```text
Question 1 ── N Turn
Turn     0..1 ── 1 Question
```

数据库层理论允许一个 Turn 暂时还没有 Question，因为这里没有 NOT NULL FK 强制关联；但一旦建立 grouping，一个 Turn 只能归属一个 Question。

顺便指出一个数据库层面的特点：**你当前这些表虽然字段具有明显的引用语义，但 DDL 并没有声明真正的 `FOREIGN KEY (...) REFERENCES ...`。** 所以目前主要依赖 application/repository 层维护 referential integrity。

---

# 六、Question 自己存了什么？

`ConversationQuestion`：

```rust
ConversationQuestion {
    id,
    session_id,
    question_index,
    title,

    question_text,
    answer_text,
    code_text,
    command_text,

    grouping_origin,

    created_at,
    updated_at,
}
```

对应：

```text
conversation_questions
```

。

这说明 Question 不只是：

```text
一组 turn_id
```

它还承担了一个很重要的职责：

### Question 是一个 materialized semantic/search unit

它把下面的信息预先聚合出来：

```text
question_text
answer_text
code_text
command_text
```

所以搜索、Memory、FTS 不需要每次：

```text
Question
 → QuestionTurn
 → Turn
 → Part
 → 拼 answer
 → 拼 code
 → 拼 command
```

而可以直接使用 Question。

你之前一直强调“Conversation 是 Memory 的基础”，从现在这个 schema 来看，其实更精确地说：

> **Session 是会话资产边界，Turn/Part 保存原始语义，而 Question 已经是为 Search / Memory 准备好的语义消费单元。**

---

# 七、Part：真正的最小持久化内容单元

Turn 往下才是 Part：

```text
Session
  Turn
    Part
    Part
    Part
```

`ConversationPart`：

```rust
ConversationPart {
    id,
    turn_id,
    part_index,

    role,
    kind,

    text,
    language,
    command,
    cwd,
    status,
    exit_code,

    command_label,
    source_execution_id,

    content_card,

    metadata_json,
    translated_text,
}
```

SQL 基础字段：

```text
conversation_parts

id
turn_id
part_index
role
kind
text
language
command
cwd
status
exit_code
metadata_json
```

然后后续 migration 又逐步加了：

```text
translated_text
content_card_json
source_execution_id
command_label
```

关系非常直接：

```text
Turn 1 ───── N Part
```

依靠：

```text
part.turn_id
```

和：

```sql
UNIQUE(tenant_id, turn_id, part_index)
```

保证一个 Turn 内的 Part 顺序。

---

# 八、Part 的 role 和 kind 是两个维度

这一点也值得单独区分。

### role

```rust
ConversationPartRole {
    User,
    Assistant,
    Tool,
    System
}
```

表示：

> 谁产生了这个 Part？

### kind

```rust
ConversationPartKind {
    Text,
    CodeBlock,
    Command,
    Tool,
    FileChange,
    Subagent,
    Metadata
}
```

表示：

> 这个 Part 是什么内容？

所以可以出现：

```text
Assistant + Text

Assistant + CodeBlock

Assistant + Tool

Tool + Tool

Tool + FileChange
```

这是两个正交维度。

---

# 九、Card：不是第五级数据库实体

Card 是目前最容易由于 UI 命名而产生错误理解的地方。

你现在没有：

```text
conversation_cards
```

这张表。

migration 做的是：

```sql
ALTER TABLE conversation_parts
ADD COLUMN content_card_json TEXT;
```

也就是说真正存的是：

```text
Part
 └── content_card_json
```

Adapter 边界对应：

```rust
ConversationContentCardDescriptor {
    schema_version,
    kind,
    renderer,
}
```

而且它存在于：

```rust
NormalizedConversationPart {
    ...
    content_card: Option<ConversationContentCardDescriptor>,
}
```

。

---

# 十、真正的 Card 是 Projection

读取之后，projection 层执行：

```rust
project_conversation_content_card(part, ...)
```

然后生成：

```rust
ConversationCard {
    card_id,
    part_id,
    adapter_id,

    kind,
    semantic_role,
    renderer,
    role,

    body,
    language,
    cwd,
    status,
    exit_code,

    source_execution_id,
    command_label,
    translated_body,
    legacy_anchor_ids,
}
```

而代码中尤其关键的是：

```rust
card_id: part.id.clone(),
part_id: part.id.clone(),
```

所以：

```text
Card ID == Part ID
```

当前模型中：

```text
Part 0 ── 1 Card
```

或者：

```text
Part 1 ── 0..1 Card
```

更准确。

因为 projection 返回的是：

```rust
Result<Option<ConversationCard>, String>
```

一个普通 Part 不一定会成为 Card。

---

# 十一、所以 Card 可以理解为 Part 的 View Model

这是我认为最准确的理解：

```text
Part = 数据事实
Card = 数据事实的展示解释
```

比如数据库 Part 可能是：

```text
role = assistant
kind = command

command = "pnpm test"
status = success
exit_code = 0

content_card_json =
{
    "schema_version": 1,
    "kind": "command",
    "renderer": "command"
}
```

它被 projection 解释为：

```text
ConversationCard
kind       = command
renderer   = Command
body       = pnpm test
status     = success
exitCode   = 0
```

所以 Card 不应该被理解成：

> Conversation 的最小原始数据单元。

真正的最小数据单元仍然是：

> **Part。**

Card 是：

> **Part 面向展示/消费侧的 projection。**

而且 `conversations/cards.rs` 本身甚至已经只是 compatibility re-export，真正实现已经移到了中立的：

```text
backend/projection/conversation_cards.rs
```

代码注释明确说这么做是为了让 persistence 不依赖 conversations domain implementation。

这个架构意图其实很清楚。

---

# 十二、为什么前端看起来是 Session → Question → Turn → Part/Card？

因为你的读取 DTO 特意做成了这个形状：

```rust
ConversationSessionDetail {
    session,
    questions: Vec<ConversationQuestionDetail>
}
```

而：

```rust
ConversationQuestionDetail {
    question,

    turns: Vec<ConversationTurn>,
    parts: Vec<ConversationPart>,
    cards: Vec<ConversationCard>,
    content_nodes: Vec<ConversationContentNode>,
}
```

所以 API 返回给 UI 的时候，它故意把数据库 graph 投影成：

```text
SessionDetail
│
├─ Session
│
└─ Questions[]
    │
    ├─ Question
    │
    ├─ Turns[]
    │
    ├─ Parts[]
    │
    ├─ Cards[]
    │
    └─ ContentNodes[]
```

这就是为什么从 React/UI 看，你非常容易产生：

```text
Session
 → Question
   → Turn
     → Part
       → Card
```

的感觉。

但这是 **Read Model**，不是底层 Domain Model。

---

# 十三、把三套结构完全拆开看，就清楚了

这是我认为你以后理解这块代码最重要的一张图。

### ① Adapter / Conversion Contract

```text
第三方 Agent 数据
        │
        ▼
NormalizedConversationSession
        │
        └── NormalizedConversationTurn[]
                  │
                  └── NormalizedConversationPart[]
                           │
                           └── content_card?
```

**这里只有 Session → Turn → Part。**

---

### ② Persistence / Domain Model

```text
ConversationSession
   │
   ├───────────────┐
   ▼               ▼
ConversationTurn   ConversationQuestion
   │                    │
   │                    ▼
   │              QuestionTurn
   │                    │
   │◄───────────────────┘
   │
   ▼
ConversationPart
   │
   ├── content_card_json
   ├── translated_text
   ├── source_execution_id
   └── metadata_json
```

这里：

```text
Question = AssetIWeave 自己建立的语义分组
```

---

### ③ Presentation / Read Projection

```text
ConversationSessionDetail
       │
       ▼
ConversationQuestionDetail
       │
       ├── Question
       ├── Turns[]
       ├── Parts[]
       ├── Cards[]
       └── ContentNodes[]
```

其中：

```text
Part
  │
  └── projection
         ↓
     ConversationCard
```

---

# 十四、用一句话给五个概念重新定义

如果以后你要给 Agent 写架构说明，我会用下面这五句话：

> **Session**：第三方 Agent 一次完整会话的资产边界。

> **Turn**：来源系统中的一次原始用户交互轮次，是同步和增量识别的重要结构单元。

> **Part**：Turn 内不可再分的标准化内容块，是 Conversation 内容的最小持久化事实单元。

> **Question**：AssetIWeave 在 Turn 之上构造的语义分组，一个 Question 可以聚合一个或多个 Turn，是搜索、展示和未来 Memory 的主要消费单元。

> **Card**：由 Part 根据 adapter card contract 投影出的展示/语义 DTO，不是独立持久化实体。

其中我尤其建议你记住这一条：

```text
                  ┌── Question = semantic grouping
                  │
Session ── Turn ──┼── Part = persisted fact
                  │
                  └── Card = projection of Part
```

**Question 往上抽象，Card 往下投影。Turn 和 Part 才是最接近来源事实的骨架。**

这其实是你目前 conversation 模型设计最核心的结构。
