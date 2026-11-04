---
name: assetiweave-memory-generation
description: AssetIWeave 记忆认知提炼工作流 SOP 与 CLI 契约说明书（增量水位过滤、批量大纲抽取、JSON 数据中枢演进提交与四态动态窗口投影）
---

# AssetIWeave Memory Generation SOP & CLI Contract

本 Skill 定义 AssetIWeave 近期工作（Recent Work）、项目记忆（Project Memory）与全局记忆（Global Memory）的标准生成流程与 CLI 工具调用契约。

---

## 0. 核心架构与最高执行红线 (Architecture & Iron Rules)

### 架构定性：JSON 唯一数据中枢 + 纯函数单向 Markdown 投影
1. **唯一数据中枢（`.state/` JSON）**：
   `~/.assetiweave/memories/<tenant>/.state/projects/<slug>.json` 是项目记忆多叉树（Topic Tree）、交互轮次开始时间戳（`turn_started_at`）、四态生命周期与会话处理水位（`session_watermarks`）的**唯一持久化真相源**；`.state/recent.json` 是按动态时间窗口切片出的近期状态缓存。
2. **零手写 Markdown（单向确定性重绘）**：
   `projects/<slug>.md` 与 `recent/latest.md` 完全由 Rust 后端根据 `.state/` JSON 数据中枢**自动按时间窗口切片并纯函数覆盖渲染**。
   **Agent 只需生成并提交结构化 JSON（调用 `aiwc memory commit --format json`），严禁手动编辑或直接写入 `projects/<slug>.md` 与 `recent/*.md`！**
3. **四态生命周期与动态窗口保留法则**：
   - `unexplored`（🍂 待探索悬空叶子）：**跨时间窗口常驻保留**。只要状态仍为 `unexplored`，无论产生于何时，均保留在 Markdown 与 GUI 中，绝不因时间窗口滑动而丢失；
   - `continued`（🌿 演进中节点）：作为树形父节点展示跨轮次/跨会话的演进脉络；
   - `completed`（✅ 已完成闭环任务）：在 `recent/latest.md` 中**严格按轮次开始时间戳 `turn_started_at` 是否落在本次时间窗口内**滚动过滤展示；
   - `dismissed`（🗑️ 已废弃/误报墓碑）：代表废案或 AI 误判。在 `.state/` JSON 中持久化保留以防后续重扫复活，但在 Markdown 与 GUI 默认视图中**自动隐藏**。

### 三条最高执行红线（违反将导致严重耗时劣化）
1. **强制 `--format json`**：
   AI 调用任何 `aiwc` CLI 命令时，**必须显式加上 `--format json` 参数**，统一解析 `{"ok": true, "data": ...}` 标准信封，严禁在默认的人类终端表格（Table）模式下调用。
2. **严禁契约摸索、源码逆向与临时采集脚本**：
   本文件第 2 节已完整给出所有命令的参数与 JSON Schema。执行过程中**严禁运行 `--help` 试探命令、严禁 `grep` 或读取仓库 Rust 源码查字段、严禁编写临时 Python 脚本循环调用 CLI**。
3. **批量分级控体积，防止挤爆上下文**：
   优先使用 `aiwc conversation list --window <W> --unprocessed --format json` 一步筛出增量会话，并用 `aiwc conversation outline <id1> <id2> ... --qa-digest --format json` 批量拉取问答摘要骨架；仅对极少数短会话或需核实细节的单个会话使用 `--qa`。

---

## 1. 标准 4 步极速执行工作流 (4-Step Fast Workflow)

一次完整的近期记忆生成应在 **3~6 次工具调用** 内完成：

### Step 1：单次调用获取时间窗口内的未处理增量会话
调用 `conversation list` 并开启 `--unprocessed` 过滤（默认窗口 `48h`，可按用户要求指定 `24h` 或 `72h`）：
```bash
aiwc conversation list --window 48h --unprocessed --format json
```
- **返回字段（`data` 数组每项）**：
  - `short_id`：8 位会话短 ID（如 `"dff13ef0"`）
  - `session_id`：完整会话 ID
  - `project_path`：项目物理路径（如 `"/Users/util6/code-space/assetiweave"`）
  - `project_slug`：后端已归一化好的项目标识（如 `"users-util6-code-space-assetiweave"`，后续命令直接使用此值，无需自行拼算）
  - `turn_count`：会话当前总轮次数（1-based 上限）
  - `watermark`：该会话在对应项目记忆中已处理到的轮次水位
  - `unprocessed_turns`：尚未处理的增量轮次数（`turn_count - watermark`）
- **0-Token 短路判定**：
  若返回的 `data` 为空数组 `[]`，说明窗口内所有会话均已处理完毕，**立即结束流程并告知用户无需更新**。

### Step 2：批量拉取增量会话问答骨架 & 读取涉及项目的既有状态
1. **批量拉取增量会话大纲**：
   将 Step 1 筛出的多个增量会话 `short_id` 一次性传给 `outline`（建议每批最多传 5~8 个 ID，首选 `--qa-digest` 摘要档防爆上下文；若单会话 `turn_count <= 5` 可直接用 `--qa`）：
   ```bash
   aiwc conversation outline <short_id_1> <short_id_2> <short_id_3> --qa-digest --format json
   ```
   - **增量关注范围**：对于 `watermark > 0` 的会话，前 `watermark` 轮作为背景参考，**重点提炼 `index > watermark` 的新增轮次**。
   - 若某轮在 `--qa-digest` 下回答被省略（显示 `... [省略 N 字] ...`）且影响关键技术结论提炼，可单独对该会话调用 `aiwc conversation outline <short_id> --qa --format json`。
2. **读取涉及项目的既有主题树状态**：
   对 Step 1 中涉及到的每个 `project_slug`，调用 `memory state get` 查看该项目现有的 `topic_trees`（重点关注现有的 `unexplored` 悬空叶子节点及其 `id`、`topic_id`、`summary`）：
   ```bash
   aiwc memory state get --project <project_slug> --format json
   ```

### Step 3：认知提炼与多叉树分支演进推理
结合各会话的增量问答与该项目既有 `topic_trees` 进行推理：

1. **判定会话是否有实质技术信号**：
   - 若某会话全篇为闲聊、简单问候或无实质结论的试探，将其归入 **无信号会话（Zero-Signal）**，仅记录到批量提交的 `advance_watermarks` 列表中推进水位，不产生树节点。
2. **与既有主题树（Topic Tree）融合演进**：
   - **接续或闭环旧悬空叶子**：若本次新轮次解决、验证或继续推进了该项目 `topic_trees` 中已有的某个节点（如 `73c20273#T1`），在提交时将 `topic_id` 设为该既有 Topic，将 `parent_node_id` 设为 `"73c20273#T1"`，并将 `parent_new_status` 设为：
     - `"completed"`：旧悬空问题已被本轮彻底解决或落地；
     - `"continued"`：本轮顺着该节点继续推进并产生新进展；
     - `"dismissed"`：本轮对话明确决定放弃、否决或废除该旧分支方案。
   - **开启新议题分支**：若属于全新的独立技术问题，命名清晰的语义化 `topic_id`（英文 kebab-case，如 `"memory-cli-convergence"`）与中文 `topic_name`（如 `"Memory CLI 契约收敛"`），`parent_node_id` 置为 `null`。
3. **严守提炼颗粒度与质量红线**：
   - ❌ **严禁照抄用户长 Prompt 原文或报错堆栈**：必须提炼为 15~45 字的单行客观技术结论。
   - ❌ **严禁收录交互过程噪音**：`"ok"`、`"继续"`、`"可以"`、`"1"`、`"按方案一办"` 等推进短语严禁作为独立节点。
   - 🎯 **单问题多方案不切碎**：若同一轮针对**同一个问题**探讨了备选方案 A/B/C 且尚未拍板，**只生成 1 条**涵盖方案概述的悬空分支（如 `"评估百万级数据存储伸缩方案（含物理分表、行级卡片打包与列瘦身）"`），严禁拆成 3 个碎片叶子。
   - 🎯 **单轮多独立问题精准分流**：若同一轮诊断出多个互不相关的独立问题（如问题 X 已顺手修复闭环，问题 Y 决定暂缓搁置），则主节点记录已闭环的问题 X（`status: "completed"`），同时在 `unexplored_branches` 中挂入未决的问题 Y。

### Step 4：单次批量提交演进 JSON（自动重绘 Markdown）与全局经验按需沉淀
1. **批量提交演进状态与水位**：
   将所有项目的演进提交项与无信号会话水位组装为一个 JSON 文件（例如写入 `/tmp/aiwc_memory_commit.json`），执行 **1 次** `memory commit` 命令：
   ```bash
   aiwc memory commit --file /tmp/aiwc_memory_commit.json --format json
   ```
   后端将自动从 SQLite 提取每个节点对应轮次的真实开始时间戳 `turn_started_at`、自动生成标准节点 ID（`<short_id>#T<turn>`）、更新 `.state/projects/<slug>.json` 与 `.state/recent.json`，并按窗口自动覆盖生成排好版的 `projects/<slug>.md` 与 `recent/latest.md`。
2. **（可选）节点 3 · 全局长期记忆 `MEMORY.md` 增量沉淀**：
   仅当本轮研发中出现了**跨项目通用、长期稳定的工程准则、架构铁律或重大环境避坑经验**时，读取并增量更新 `~/.assetiweave/memories/<tenant>/MEMORY.md`（控制在 100 行以内，严禁写入单项目业务功能进展、具体 Issue 流水或单次测试指标）。

---

## 2. CLI 工具说明书与精确 JSON 契约 (CLI Reference & Schemas)

> **注意**：以下所有命令均必须携带 `--format json`。

### 2.1 `aiwc conversation list` —— 查询窗口内增量会话
```bash
aiwc conversation list --window 48h --unprocessed --format json
```
输出信封示例：
```json
{
  "ok": true,
  "data": [
    {
      "short_id": "dff13ef0",
      "session_id": "conversation-session-dff13ef0...",
      "updated_at": "2026-11-03T15:14:38.860Z",
      "turn_count": 3,
      "watermark": 1,
      "unprocessed_turns": 2,
      "project_path": "/Users/util6/code-space/assetiweave",
      "project_name": "assetiweave",
      "project_slug": "users-util6-code-space-assetiweave"
    }
  ]
}
```

### 2.2 `aiwc conversation outline` —— 批量或单会话大纲抽取
```bash
# 批量抽取多个会话的问答摘要（推荐主力档）
aiwc conversation outline <id1> <id2> --qa-digest --format json

# 单会话抽取完整问答（仅在短会话或摘要不足以判断结论时使用）
aiwc conversation outline <id1> --qa --format json
```
- **列序契约（`--qa-digest` 与 `--qa` 完全相同，共 8 列）**：
  `columns = ["index", "turn_id", "question", "answer", "turn_kind", "agent_role", "parent_turn_id", "started_at"]`
  - `index`（列 0）：**1-based** 轮次序号（提交 `commit` 时的 `turn_index` 必须严格使用此序号）。
  - `turn_id`（列 1）：轮次 8 位短 ID。
  - `question`（列 2）：用户提问原文。
  - `answer`（列 3）：AI 最终回复（`--qa-digest` 下超过 500 字会自动保留前 200 字与后 300 字）。
  - `turn_kind` / `agent_role` / `parent_turn_id`（列 4~6）：轮次种类与子代理父轮次指针。
  - `started_at`（列 7）：该交互轮次真实的开始时间戳（RFC3339）。
- **单 ID 与多 ID 返回结构差异**：
  - 传入 1 个 ID 时：`data` 直接为 `{"level": "session", "session_id": "...", "total_turns": 3, "turns": {"columns": [...], "rows": [...]}}`。
  - 传入多个 ID 时：`data` 为批量容器 `{"level": "batch", "items": [ {"level": "session", ...}, ... ], "truncated": false}`。

### 2.3 `aiwc memory state get` —— 读取既有项目或近期状态树
```bash
# 读取指定项目的 .state/projects/<slug>.json（支持传 project_slug 或项目物理路径）
aiwc memory state get --project <project_slug_or_path> --format json

# 读取当前工作目录所属项目的状态树
aiwc memory state get --current-project --format json
```
返回该项目已有的 `topic_trees`（各 Topic 下的节点 `id`、`parent_id`、`summary`、`status`、`turn_started_at`）以及 `session_watermarks`。

### 2.4 `aiwc memory commit` —— 提交演进节点与推进水位（核心写接口）
推荐使用 `--file <json_path> --format json` 一次性批量提交。

#### 批量提交 JSON 标准模板（Agent 唯一需要构造的结构）：
```json
{
  "window": "48h",
  "commits": [
    {
      "project": "users-util6-code-space-assetiweave",
      "session_id": "dff13ef0",
      "turn_index": 3,
      "topic_id": "memory-cli-convergence",
      "topic_name": "Memory CLI 契约收敛与性能优化",
      "parent_node_id": "21c6034c#T1",
      "parent_new_status": "continued",
      "status": "completed",
      "summary": "确立 CLI 零 AI 纯读写定位，移除冗余子命令并支持批量大纲采集 (#174)",
      "unexplored_branches": [
        {
          "turn_index": 2,
          "summary": "重构桌面端 Recent Work 前端界面以直连 .state JSON 并提供废案一键清理按钮"
        }
      ]
    }
  ],
  "advance_watermarks": [
    {
      "project": "users-util6-code-space-assetiweave",
      "session_id": "a1b2c3d4",
      "turn": 4
    }
  ]
}
```

#### 字段说明与后端自动补齐规则：
| 字段路径 | 必填 | 类型与取值说明 |
| :--- | :--- | :--- |
| `window` | 可选 | 本次刷新 `recent/latest.md` 的时间窗口（如 `"24h"`、`"48h"`、`"72h"`，默认 `"48h"`） |
| `commits[].project` | **必填** | 直接填入 `conversation list` 返回的 `project_slug` 或 `project_path` |
| `commits[].session_id` | **必填** | 支持直接填 8 位 `short_id`（如 `"dff13ef0"`）或完整 `session_id`，后端自动解析 |
| `commits[].turn_index` | **必填** | 产生该结论的 **1-based** 轮次序号（对应 `outline` 的 `index` 列） |
| `commits[].topic_id` | **必填** | 议题英文标识（如 `"memory-cli-convergence"`）；若接续既有 Topic 则填既有 `topic_id` |
| `commits[].topic_name` | 推荐 | 议题人类可读中文名称（如 `"Memory CLI 契约收敛"`） |
| `commits[].parent_node_id` | 可选 | 若接续/闭环/废弃既有节点，填既有节点 ID（如 `"21c6034c#T1"`）；新议题根节点填 `null` 或省略 |
| `commits[].parent_new_status` | 可选 | 当指定 `parent_node_id` 时，父节点的新状态：`"continued"`（默认）、`"completed"` 或 `"dismissed"` |
| `commits[].status` | **必填** | 当前新节点状态：`"completed"`（已闭环）、`"continued"`（推进中）或 `"unexplored"`（待探索悬空） |
| `commits[].summary` | **必填** | 单行精炼技术结论（15~45 字，**严禁手写 `[short_id#Tn](aiwc://...)` 锚点前缀**，投影器会自动生成锚点与时间戳！） |
| `commits[].unexplored_branches` | 可选 | 同轮次或同会话衍生出的待探索悬空分支数组，每项含 `turn_index`（可选，默认同主节点）与 `summary`（单行描述，同样**不要**手写锚点前缀） |
| `advance_watermarks[]` | 可选 | 针对无信号会话或最后一轮高于 `commit.turn_index` 的会话，将其水位推进至最新 `turn_count`（含 `project`, `session_id`, `turn`） |

> **后端自动完成的事项（Agent 严禁自行冗余处理）**：
> - 无需手拼 `<short_id>#T<turn>` 或 `.1/.2` 子节点 ID，后端自动按规范生成；
> - 无需手查或手填 `turn_started_at`、`created_at`、`updated_at`，后端自动从 SQLite 会话轮次表读取真实的 `turn_started_at` 注入节点；
> - 无需手动渲染或写回 `projects/<slug>.md` 与 `recent/latest.md`，`memory commit` 执行完毕后已原子同步完成。

### 2.5 `aiwc memory status` —— 单节点状态修正与废案清理
当用户指出某条悬空叶子“其实已经做完（AI 误判）”或“是个废案、以后不再考虑”时，调用此命令直接翻转节点状态并重绘 Markdown：
```bash
# 将废案或无价值悬空分支标记为 dismissed（保留 JSON 墓碑防重入，并立即从 Markdown 悬空列表中隐藏）
aiwc memory status <node_id> dismissed --format json

# 将 AI 误判为悬空的已完成事项修正为 completed
aiwc memory status <node_id> completed --format json
```

### 2.6 `aiwc memory recent get` / `aiwc memory project get` —— 读取生成的 Markdown 视图
```bash
aiwc memory recent get --format json
aiwc memory project get --current-project --format json
```

---

## 3. 提炼正反例速查表 (Extraction Do's & Don'ts)

| 场景 | ❌ 错误做法（严禁） | ✅ 正确做法（标准） |
| :--- | :--- | :--- |
| **用户长指令输入** | `summary` 照抄用户命令：`"/implement-spec 落地 #174 及其所有测试"` | 提炼最终闭环成果：`"收敛 Memory CLI 纯读写命令面并支持批量大纲与四态窗口投影 (#174)"` |
| **锚点前缀重复** | 在 `summary` 字段内手写 `"[dff13ef0#T3](aiwc://...): 确立 CLI..."`（会导致投影器双重嵌套锚点） | `summary` 只写纯文本结论 `"确立 CLI 零 AI 定位..."`，由后端投影器统一冠上锚点与 `turn_started_at` 时间戳 |
| **单问题多备选方案** | 同一问题讨论了方案 A/B/C 未定，在 `unexplored_branches` 里拆成 3 条平级叶子 | 收敛为 **1 条**问题级悬空叶子：`"评估百万级数据存储伸缩方案（涵盖物理分表、行级打包与列瘦身）"` |
| **单轮多独立问题部分闭环** | 把已修好的前端按钮违规和未修的 800 行超限问题混为一谈记为完成 | 已修好的记入主节点 `status: "completed"`，未修的文件超限问题放入 `unexplored_branches` |
| **历史悬空叶子在新会话闭环** | 不查 `memory state get`，直接新建孤立节点，导致旧悬空叶子永远挂在池中 | 查出旧叶子 ID（如 `"73c20273#T1"`），填 `parent_node_id: "73c20273#T1"`, `parent_new_status: "completed"` |
