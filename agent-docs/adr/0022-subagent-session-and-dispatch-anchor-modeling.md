# 0022: 子代理会话与父轮次委派锚点建模

> **状态**：已接受（取代 #148 将子代理拆分为 `turn_kind = "subagent" | "continuation"` 伪轮次的约定，扩展 ADR-0006、ADR-0013 与 ADR-0021；其"父快照替换顺带清理子会话/维护全文索引"的表述由 ADR-0023 收窄为单一索引与保险箱语义）
> **决策日期**：2026-11-09

此前我们将子代理（Subagent）建模为父会话内的特殊交互轮次（`turn_kind = "subagent"`）及紧随其后的续接伪轮次（`turn_kind = "continuation"`），导致一次真实的人类提问在派发 $K$ 个子代理时被物理切碎为 $2K + 1$ 行 `conversation_turns`（例如 1 次提问拆成 7 个物理 Turn），不仅使后续真实提问的物理轮次号发生严重偏移（`#2` 变成 `#T8`）、污染 Memory 水位计数，还使 `aiwc conversation outline` 顶层大纲被数千字的子代理委派 Prompt 刷屏。

我们决定采用**零数据库表结构变更（0 DDL）**的“**子代理会话（Subagent Session）+ 父轮次子代理委派锚点（Subagent Dispatch Anchor）**”模型：
1. **子代理本身作为独立从属会话（`Subagent Session`）**：每个子代理均持久化为独立的 `conversation_sessions` 行，直接复用现有列标记 `user_visible = 0`、`execution_origin = "subagent"`，并在 `execution_purpose` 中承载父子溯源元数据（`parent_session_id`、`agent_role`、`spawn_anchor`）。其内部首轮（`#T1`）完整承载父代理下发的全量委派 Prompt、工具调用链与最终汇报。
2. **父轮次保持完整（1 问 = 1 Turn）并内嵌委派锚点（`Subagent Dispatch Anchor`）**：彻底废除 `turn_kind = "subagent" | "continuation"` 拆轮机制；父会话的交互轮次在第 $k$ 步派发子代理时，在 `TurnProcess.items` 与 `conversation_parts`（`kind = "subagent"`，元数据存于现有 `attrs_json` 列）中记录轻量委派锚点（含确定性预计算的 `child_session_id`、角色、任务标题、状态与结论摘要）。

## 备选方案与取舍

- **方案 A：保持特殊 `Turn`（`turn_kind = "subagent"`），仅在 CLI/UI 层过滤隐藏**：虽然无需抽离会话，但无法消除 `continuation` 续接切分导致的物理 `turn_index` 膨胀，父轮次的正文回答仍被迫挂在最后一个伪 Turn 上，违背 ADR-0006“一问一轮”与 ADR-0021 位置身份的语义一致性。
- **方案 B：纯内联特殊 `Part`（不建子 Session，把子代理全量步骤塞进父 Turn 的单个 `Part`）**：虽然父会话轮次号干净，但子代理内部往往包含数十步工具调用乃至嵌套子代理，全部塞入单个 Part 的 JSON 载荷会丧失独立会话大纲（`aiwc conversation outline <child_session_id>`）、卡片定位与按需懒加载能力。
- **方案 C（选定）：独立 `Subagent Session` + 父 Turn 轻量委派锚点 `Part`**：既保证父会话“1 次人类提问 = 1 个完整 Turn”、首屏加载零 N+1 查询，又让子代理天然复用整套 `Session -> Turn -> Part` 投影、大纲与卡片下钻能力。

## 后果

- **跨会话同步与确定性预锚定（兼容 ADR-0021）**：由于会话主键由 `stable_id("conversation-session", &[source_id, external_id])` 纯函数确定性生成，父会话锚点与子会话行无需等待对方先入库即可双向算出确定性 ID；五大宿主适配器（Claude Code、CodeBuddy、Antigravity、OpenCode、Codex）均支持单次解析产出一主多子 `NormalizedConversationSession`。父会话发生 `/rewind` 回滚并执行整包快照替换时，对账不再被引用的孤儿子会话（若磁盘源文件亦不存在则级联清理）。
- **双层 FTS5 全文检索与溯源跳转**：父轮次委派锚点将 `[agent_role + 任务标题 + 汇报摘要]` 写入父会话 FTS5 索引；会话列表全文检索同时匹配主会话及其名下 `Subagent Session` 并向上归集浮现父会话；卡片级检索命中子会话内部时附带父会话上下文，支持 CLI 与前端带 `[← 返回父会话 #Tn]` 面包屑直跳。
- **存量数据无损原地还原**：现有 SQLite 表结构无需任何 DDL 变更；存量库中被拆碎的 `subagent` / `continuation` 伪轮次与平铺的 Codex 子代理会话可通过纯数据层折算（抽离 `subagent` 轮次为子会话、生成父轮次锚点、将 `continuation` 轮次缝合回父轮次并重排 `0..N` 位置主键）无损恢复。
- **锚点身份只认 `child_session_id`（2026-11-10 实现注记）**：同一锚点会同时写入 `conversation_parts`（正文）与 `TurnProcess.items`（过程链）。存量折算路径让二者**各占一个序号**（实测 `part_index = 21` / `seq = 22`，恒差 1），新扫描路径则恰好相等。因此任何渲染或检索消费方都必须按 `child_session_id` 对齐；一旦假设「`seq` 即 `part_index`」，在折算数据上会整体错位一格。
