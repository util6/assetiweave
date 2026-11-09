---
name: assetiweave-memory
description: 通过 AssetIWeave 的纯读写 Memory CLI 与会话逐级取证体系（批量 Outline 骨架还原、问答摘要透视、卡片按需单点下钻）查询近期工作、历史会话、项目悬空分支并管理节点状态。
---

# AssetIWeave Memory

所有操作均通过 AssetIWeave 原生 CLI（`aiwc`）纯读写契约完成。Agent 不直接读写底层 SQLite，也不绕过 CLI 直接修改 `.state/` 内部文件。

## 0. AI 调用最高铁律 (Mandatory JSON Output Rule)

- **所有命令强制携带 `--format json`**：
  AI 调用任何 `aiwc` CLI 命令时，**必须显式附加 `--format json` 参数**，统一解析标准 JSON 信封 `{"ok": true, "data": ...}`（错误时为 `{"ok": false, "error": ...}`）。严禁在不带 `--format json` 的默认终端表格（Table）模式下调用。
- **乐观执行与零试探原则**：
  默认假设运行时正常，直接按本文档契约调用业务命令；**严禁在启动时执行 `doctor` 或 `--help` 试探命令**。

---

## 1. 逐级取证事实求证协议 (Progressive Evidence & Fact Verification Protocol)

### 双入口取证流 (Dual-Entry Verification Flow)
用户的使用场景分为两种入口，**两者必须最终无缝汇入 4-Tier 逐级取证链路**：

1. **入口 A：精确短 ID 入口 (Direct Short-ID Entry)**
   - 当用户 Prompt、问题输入或记忆锚点中已提供 8 位十六进制短 ID（例如 `dff13ef0`）或完整 Session/Turn/Card ID 时（支持一次传入 1 个或多个 ID）：
   - **第一步直接调用**骨架还原命令：
     ```bash
     aiwc conversation outline <id1> [<id2> ...] --qa-digest --format json
     ```

2. **入口 B：自然语言模糊回忆与近期工作入口 (Natural Language Recall Entry · 用户核心场景)**
   - 当用户用自然语言提问回忆历史事实、近期研发进展、待探索悬空分支或排查经过时：
   - **第 1 步：快速定位候选会话或记忆分支**：
     ```bash
     # 按关键词跨会话检索命中卡片
     aiwc conversation search --query "<关键词>" --limit 10 --format json

     # 或查看近期窗口内的活跃会话列表（含 project_slug 与水位）
     aiwc conversation list --window 48h --format json

     # 或读取近期工作 / 当前项目记忆视图（内含 [short_id#Tn] 协议锚点）
     aiwc memory recent get --format json
     aiwc memory project get --current-project --format json
     ```
   - **第 2 步：提取 Session 短 ID 并立刻衔接取证**：
     一旦从搜索命中结果、会话列表或记忆锚点（`[<short-id>#T<turn>]`）中定位到候选会话，**必须当场提取其 8 位短 ID，立即无缝触发 Tier 0 骨架还原**（若有多个候选短 ID，可一次批量传入）：
     ```bash
     aiwc conversation outline <short-id-1> [<short-id-2> ...] --qa-digest --format json
     ```
   - **严禁二手摘要交差 (No Second-hand Summary)**：
     对于任何涉及技术方案、Bug 修复细节、关键配置或历史讨论的提问，**严禁仅凭记忆文件中的单行结论草草回复**！必须在第 1 轮对话内下钻到会话骨架或卡片正文完成核实，将原始验证细节连同 **Session 短 ID 与轮次时间** 一同交付给用户。

---

### 命令拓扑 (Agent-Facing Command Map)

| 目的 | 命令（均须携带 `--format json`） | 说明 |
| --- | --- | --- |
| **大纲骨架（支持批量）** | `aiwc conversation outline <id1> [<id2> ...] [--questions \| --qa-digest \| --qa \| --full] --format json` | 按 ID 类型自适应返回 **session / turn / card** 三级骨架（多 ID 时返回 `level: "batch"`），含轮次真实开始时间 `started_at` |
| **单卡正文** | `aiwc conversation card get <card_id> [--raw] --format json` | O(1) 直读单张卡片正文；加 `--raw` 直接输出纯文本到 stdout |
| **定位会话** | `aiwc conversation list [--window 48h] [--unprocessed] [--current-project] [--query <q>] [--parent <session-id>] --format json` | 返回会话列表及 `project_slug`、`watermark`、`unprocessed_turns`；`--query` 会把隐藏子代理会话内部的命中归集到可见父会话，`--parent` 则枚举指定父会话名下的子代理会话（支持 8 位短 ID） |
| **跨会话检索** | `aiwc conversation search --query "<q>" [--limit 20] --format json` | 按关键词检索命中的会话卡片与摘要片段；命中子代理会话内部时每条 hit 附带 `parent_session_id` 与 `parent_turn_index` 返回面包屑 |
| **近期工作视图（读）** | `aiwc memory recent get [--date <YYYY-MM-DD>] --format json` | 读取 `recent/latest.md`（或指定日期快照），查看近期闭环任务与常驻悬空叶子 |
| **项目记忆视图（读）** | `aiwc memory project get [<project_path> \| --current-project] --format json` | 读取目标项目的 `projects/<slug>.md` 记忆视图 |
| **结构化状态树（读）** | `aiwc memory state get [--project <path_or_slug> \| --current-project] --format json` | 读取 `.state/projects/<slug>.json`（不传项目时读 `.state/recent.json`）强类型多叉树与水位 |
| **节点状态修正（写）** | `aiwc memory status <node_id> <completed\|dismissed\|unexplored\|continued> [--project <slug>] --format json` | 将指定节点（如 `73c20273#T1`）标为已完成或废案（`dismissed`），并自动重绘 Markdown |
| **演进提交（写）** | `aiwc memory commit --file <json_path> --format json` | 批量或单条提交主题树演进节点与水位（详见 `assetiweave-memory-generation` Skill） |

---

### 输出形态：列式 Matrix JSON (Columnar Matrix)

所有骨架统一为 `{"columns": [...], "rows": [[...]]}`：列名只声明一次，行是**定长元组数组**。必须严格按 **columns 的列序**取值。

#### 1. Session 级（`level: "session"`）
顶层字段：`session_id`、`source_id`、`total_turns`、`created_at?`、`turns { columns, rows }`；若被查看的会话本身是 `Subagent Session`，还会额外输出 `parent_session_id`（父会话 8 位短 ID）与 `agent_role`（子代理语义角色）面包屑。
支持四档互斥详细度开关与默认骨架（末尾均包含轮次真实开始时间 `started_at`）。**`turn_kind` / `agent_role` / `parent_turn_id` 三个伪轮次列已物理删除**，一律按 `columns` 列序取值，不要按旧列号硬编码：
- **默认（不带开关，6 列）**：
  `turns.columns = ["index", "turn_id", "question", "activities", "cards", "started_at"]`
- **`--questions`（最精简，4 列）**：
  `turns.columns = ["index", "turn_id", "question", "started_at"]`
- **`--qa-digest`（问答摘要主力档，防爆上下文，5 列）**：
  `turns.columns = ["index", "turn_id", "question", "answer", "started_at"]`
  返回完整 `question` 与首尾高信号截取的 `answer`（超过 500 字时保留前 200 字与后 300 字），非常适合多会话批量快速浏览。
- **`--qa`（完整问答档，5 列）**：
  `turns.columns = ["index", "turn_id", "question", "answer", "started_at"]`
  返回未截断的最终回答内容（当批量请求总体积超过 64 KB 安全预算时会自动降级截断并标记 `truncated: true`）。
- **`--full`（最详细，7 列）**：
  `turns.columns = ["index", "turn_id", "question", "activities", "cards", "answer", "started_at"]`

列语义说明：
- `index`：**1-based** 轮次序号。
- `activities`：该轮工具/命令活动的**连续去重**摘要（如 `["Git status check", "Build desktop crate"]`）；末尾可能追加子代理委派段 `Subagent(<role> -> <child_short_id>)`，多个锚点折叠为 `Subagent xN(...)`。
- `cards`：该轮卡片数量（整数），据此判断是否需要下钻到 Turn 级。
- `started_at`：该交互轮次真实的开始时间戳（RFC3339）。

子代理不再以伪轮次并列出现在父会话大纲里（1 次人类提问 = 1 个 Turn）。下钻方式：从 `activities` 里的 `Subagent(...)` 或 Turn 级 `cards` 中 `kind` 以 `subagent` 结尾的卡片取 `child_session_id`，再用 `aiwc conversation outline <child_session_id>` 查看子会话；也可用 `aiwc conversation list --parent <parent_session_id>` 一次枚举全部子代理会话。

#### 2. 批量模式（`level: "batch"`）
当向 `aiwc conversation outline` 传入 2 个及以上 ID 时，`data` 返回：
```json
{
  "level": "batch",
  "items": [
    { "level": "session", "session_id": "dff13ef0", ... },
    { "level": "session", "session_id": "21c6034c", ... }
  ],
  "truncated": false
}
```

#### 3. Turn 级（`level: "turn"`）
顶层字段：`session_id`、`turn_id`、`turn_index`、`total_turns`、`user_question`、`started_at`，以及：
`cards.columns = ["order", "card_id", "kind", "label", "status", "chars", "child_session_id"]`
- `label`：命令/工具的语义标签，是判断卡片价值的最强信号；委派锚点卡片取锚点标题。
- `chars`：正文长度预估。**先看 `chars` 再决定是否下钻**，避免把超长输出拉进上下文。
- `child_session_id`：仅 `kind` 以 `subagent` 结尾的 `Subagent Dispatch Anchor` 非空，为子会话 8 位短 ID，可直接下钻；其余卡片为 `null`。

#### 4. Card 级（`level: "card"`）
顶层字段：`session_id`、`turn_id`、`turn_index`、`turn_user_question`、`target { card_id, order, kind, label?, status?, chars }`，以及邻域矩阵 `neighborhood.columns = ["order", "card_id", "kind", "label", "is_target"]`。

---

### 4 级逐级取证梯级 (4-Tier Progressive Evidence Protocol)

1. **Tier 0 · 会话骨架**：`aiwc conversation outline <session-id> [--qa-digest] --format json` → 一轮一行的全景地图。
2. **Tier 1 · 意图与结论透视**：扫描 `question` 与 `answer` 摘要列（及 `started_at` 时间戳），多数事实追溯与方案回顾在此即可定论。
3. **Tier 2 · 门禁判定**：
   - **推断充足** → 立即输出结论，**严禁多余下钻**；
   - **事实缺失**（需要完整回答、代码 diff、堆栈或具体命令输出）→ 进入 Tier 3。
4. **Tier 3 · 逐级放大（禁止盲目全量 dump）**：
   ```bash
   aiwc conversation outline <session-id> --qa --format json   # 查看该会话完整回答
   aiwc conversation outline <turn-id> --format json           # 查看该轮卡片清单：kind/label/status/chars
   aiwc conversation card get <card-id> --format json          # O(1) 获取目标单卡精确正文
   ```
   - 每次仅 `card get` 1~2 张关键卡片，查完即止。

---

## 2. 严格防逃逸与爬虫熔断规约 (Anti-Crawling & Circuit Breaker)

- **翻页深度熔断 (Pagination Hard Limit)**：
  调用 `aiwc conversation search` 时，**严禁无节制深度翻页**！`--offset` 参数**严禁超过 50**（最多查看 2 页）。
- **逃逸熔断 (Search Escape Circuit Breaker)**：
  若在 2 页（`offset <= 50`）内仍未找到目标记录，必须**立即熔断停止检索**，向用户如实反馈并引导提供更具体的关键词、时间范围或 8 位短 ID。

---

## 3. 记忆节点状态管理（修正误判与清理废案）

记忆树中的节点具有四种状态：
- `unexplored`（🍂 待探索悬空叶子）：跨时间窗口常驻保留在 Markdown 与 GUI 中；
- `continued`（🌿 演进中）：中间演进节点；
- `completed`（✅ 已完成闭环）：在 Recent 视图中按 `turn_started_at` 时间窗口滚动展示；
- `dismissed`（🗑️ 已废弃/误报墓碑）：在 `.state/` JSON 中保留墓碑防止后续重跑再次提取，但在 Markdown 与 GUI 主视图中自动隐藏。

当用户指示某个悬空叶子“其实已经解决”或“是个废案、以后不再考虑”时，直接调用：
```bash
# 标记为废案/忽略（立即从 recent/latest.md 与 projects/<slug>.md 悬空列表中移除）
aiwc memory status <node_id> dismissed --format json

# 标记为已完成闭环
aiwc memory status <node_id> completed --format json
```

---

## 4. 开发模式与瞬时锁重试指引

- **开发模式环境变量**：
  在 `assetiweave` 源码工作区内开发联调时，设置 `export ASSETIWEAVE_ENV=development` 以锁定 `./target/debug/` 编译产物。
- **SQLite 瞬时占用重试**：
  若遇到 `error returned from database: (code: 5) database is locked`，说明桌面版正在写库，属瞬时占用；直接间隔 3~5 秒重试同一命令即可，不要放弃取证。
