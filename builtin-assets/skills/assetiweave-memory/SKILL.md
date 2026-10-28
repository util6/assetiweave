---
name: assetiweave-memory
description: 通过 AssetIWeave 的统一 Memory API 与会话逐级取证体系（Outline 骨架还原、意图透视、卡片按需单点下钻）查询近期工作、历史会话与项目上下文。
---

# AssetIWeave Memory

所有操作都通过 AssetIWeave CLI 的 Engine 合同完成。脚本不直接读写底层 SQLite，不绕过 Engine 访问第三方会话数据库，也不自行决定工具权限。

## 1. 逐级取证事实求证协议 (Progressive Evidence & Fact Verification Protocol)

### 乐观执行原则 (Optimistic Execution)
- 默认假设运行时与合同环境正常，直接调用各业务命令执行，**严禁在启动时强制执行 `doctor` 诊断**！
- 只有在遇到不可恢复的环境报错、CLI 缺失或版本/契约断裂时，才跳转至文末的“故障排查”章节。

### 双入口取证流 (Dual-Entry Verification Flow)
用户的使用场景分为两种入口，**两者必须最终无缝汇入 4-Tier 逐级取证链路**：

1. **入口 A：精确短 ID 入口 (Direct Short-ID Entry)**
   - 当用户 Prompt、问题输入或上下文中已提供 8 位十六进制会话短 ID（例如 `2e003a42`）或完整 Session ID 时：
   - **第一步直接调用**骨架还原命令：
     ```bash
     aiwc conversation outline <session-id>
     ```

2. **入口 B：自然语言模糊回忆入口 (Natural Language Recall Entry · 用户核心场景)**
   - 当用户用自然语言提问回忆历史事实、排查经过或技术方案时，用户**不可能预先知道短 ID**。
   - **第 1 步：快速定位**：
     ```bash
     aiwc memory context resolve --current-project --query "<关键词/描述>" --token-budget 2000
     ```
     （或使用 `aiwc conversation search --query "<关键词>" --current-project --limit 5`）
   - **第 2 步：提取 Session 短 ID 并立刻衔接取证**：
     `context resolve` 输出包含明确的会话标记 `## Session Memory [Session: <short-id>]`（且 references 中附带 `session_id`）。
     **强制规则**：一旦定位到候选会话，**必须当场提取其 8 位短 ID，并立即无缝触发 Tier 0 骨架还原**：
     ```bash
     aiwc conversation outline <short-id>
     ```
   - **严禁二手摘要交差 (No Second-hand Summary)**：
     对于任何涉及技术方案、Bug 修复细节、关键配置或历史讨论的提问，**严禁仅凭 `context resolve` 的 3 行粗略摘要草草回复**！必须在第 1 轮对话内完成取证核实，将原始验证细节连同 **Session 短 ID** 一同交付给用户！

### 命令拓扑 (Agent-Facing Command Map)

| 目的 | 命令 | 说明 |
| --- | --- | --- |
| 骨架（唯一入口） | `aiwc conversation outline <id>` | 按 ID 类型自适应返回 **session / turn / card** 三级骨架，支持 8 位短 ID。Session 级支持 `--questions` / `--qa` / `--full` 详细度选项 |
| 单卡正文 | `aiwc conversation card get <card_id>` | O(1) 直读正文；`--raw` 直接输出纯文本到 stdout |
| 定位会话 | `aiwc conversation list [--window 24h]` | 人读表格，用于拿到短 ID |
| 跨会话检索 | `aiwc conversation search --query <q>` | 关键词命中卡片 |
| 记忆上下文（组装） | `aiwc memory context resolve --current-project [--query <q>] [--token-budget 2000]` | 从 `MEMORY.md`、当前项目的 `projects/<slug>.md` 与 `recent/latest.md` 组装返回有界 Markdown 上下文 |
| 近期工作（Recent） | `aiwc memory recent get [--date <YYYY-MM-DD>]` | 直读 `recent/latest.md`（或指定日期文件），查看最新思维分支与闭环任务 |
| 项目记忆（Project） | `aiwc memory project get <project_path> / --current-project` | 直读 `projects/<slug>.md` 悬空叶子池，查看当前项目尚未探索的思维分支 |

- `card get` 默认输出 `{card_id, session_id, turn_id, kind, label?, status?, content}`；加 `--raw` 则只吐 `content` 纯文本，适合管道与代码/日志提取。

### 输出形态：列式 Matrix JSON (Columnar Matrix)

所有骨架统一为 `{"columns": [...], "rows": [[...]]}`：列名只声明一次，行是**定长元组数组**。
必须按 **columns 的列序**取值，不要指望行内出现字段名。

**Session 级**（`level: "session"`）：`session_id`、`source_id`、`total_turns`、`created_at?`。
支持三档互斥详细度开关与默认骨架：
- 默认（不带开关）：`turns.columns = ["index", "turn_id", "question", "activities", "cards"]`
- `--questions`（最精简）：`turns.columns = ["index", "turn_id", "question"]`，仅输出用户问题主干。
- `--qa`（问答主干，思维分支寻回主力档）：`turns.columns = ["index", "turn_id", "question", "answer"]`，直接返回最终回答内容（`turn.response`），不含 activities 与 cards 噪音。
- `--full`（最详细）：`turns.columns = ["index", "turn_id", "question", "activities", "cards", "answer"]`，包含卡片/活动概况与最终回答。
- `activities`：该轮工具/命令活动的**连续去重**摘要（如 `["Git status check","Build desktop crate"]`），不是逐条流水。
- `cards`：该轮卡片数量（整数），据此判断是否需要放大到 Turn 级。
- `index` 为 **1-based** 轮次序号。

**Turn 级**（`level: "turn"`）：`session_id`、`turn_id`、`turn_index`、`total_turns`、`user_question`，以及
`cards.columns = ["order","card_id","kind","label","status","chars"]`
- `label`：命令/工具的语义标签（Agent 设计里的 command_label），是判断卡片价值的最强信号。
- `chars`：正文长度预估。**先看 chars 再决定是否下钻**，避免把超长输出拉进上下文。
- 本层**绝不输出正文**。

**Card 级**（`level: "card"`）：`session_id`、`turn_id`、`turn_index`、`turn_user_question`，以及
`target { card_id, order, kind, label?, status?, chars }`，以及
`neighborhood.columns = ["order","card_id","kind","label","is_target"]`
- 邻域给出该卡在轮内的前后文坐标，用 `is_target` 定位自身。

**统一信封与体积基线**
- 所有命令输出统一为 `{"ok": true, "data": ...}`，`data` 才是骨架主体。
- `outline` / `card get` **恒为 JSON**，`--format` 对它们无效，不要传（传了也不会改变形态）。
- 体积参考：20 轮左右会话的 Session 级骨架约 **10~15 KB**，可直接整段读入；再往上放大才需要预算控制。

### 4 级逐级取证梯级 (4-Tier Progressive Evidence Protocol)

1. **Tier 0 · 会话骨架**：`aiwc conversation outline <session-id>` → 一轮一行的全景地图。
2. **Tier 1 · 意图透视**：只读 `question` 列自顶向下扫一遍，多数事实追溯/方案回顾在此即可定论。
3. **Tier 2 · 门禁判定**：
   - **推断充足** → 立即输出结论，**严禁多余下钻**；
   - **事实缺失**（需要 diff、堆栈、配置、精确回复）→ 才进入 Tier 3。
4. **Tier 3 · 逐级放大（三级缩放进阶，禁止跳级 dump）**：
   ```bash
   aiwc conversation outline <turn-id>    # 该轮卡片清单：kind/label/status/chars
   aiwc conversation outline <card-id>    # 单卡邻域坐标（可选，用于确认上下文）
   aiwc conversation card get <card-id>   # 仅此时才取正文
   ```
   - **严禁全量 dump 会话正文**，每次仅 `card get` 1~2 张关键卡片，查完即止。
   - 优先按 `label` 与 `chars` 选卡：语义标签明确、体积适中的卡片命中率最高。

### 开发模式环境指引 (Development Mode Guidance)
在 `assetiweave` 源码工作区内开发联调时，设置环境变量：
```bash
export ASSETIWEAVE_ENV=development
```
设置后，CLI、Engine 与脚本将强制锁定工作区 `./target/debug/` 编译产物，杜绝与系统全局安装版本冲突。

## 2. 严格防逃逸与爬虫熔断规约 (Anti-Crawling & Circuit Breaker)

- **翻页深度熔断 (Pagination Hard Limit)**：
  调用 `aiwc conversation search` 时，**严禁无节制深度翻页**！
  `--offset` 参数**严禁超过 50**（即最多只允许查看第 1 页 offset 0 和第 2 页 offset 25/50，上限 2 页）。
- **逃逸熔断 (Search Escape Circuit Breaker)**：
  如果在 2 页（offset <= 50）内仍未找到目标记录，必须**立即熔断停止检索**，向用户如实反馈未检索到相关内容，并引导用户提供更具体的关键词、时间范围或 8 字符短 ID。**严禁以自作主张的机械翻页替代向用户反馈**。
- **范围与边界意识**：
  结果严格带有租户和范围（scope）边界，严禁跨租户拼接结果；当指定范围返回为空时，说明该范围无记录，不擅自扩大到全局范围。

## 3. 常规 Memory 读取与上下文编译

### 最近工作
```bash
aiwc memory recent get
```
需要限定当前项目时增加 `--current-project`。

### 编译上下文 (Context Resolve)
```bash
aiwc memory context resolve --current-project --query "<主题>" --token-budget 2000
```
上下文为有界投影。回答时必须区分返回的直接内容与基于内容作出的推断，并保留 `revision`。

### 项目投影与维护重建
```bash
aiwc memory project get "<PROJECT_PATH>"
aiwc memory rebuild --target project --project "<PROJECT_PATH>"
aiwc memory rebuild --target global
aiwc memory rebuild --target recent
aiwc memory rebuild --target all
aiwc memory rebuild --target all --reason projection_repair
aiwc memory task list --active-only
```
重建立即返回任务状态快照；使用 `aiwc memory task get <task-id>` 跟踪，不阻塞宿主。

## 4. Recall 结构化多轮工作流

### 快速单次检索
```bash
aiwc memory recall search --query "<QUERY>" --current-project --limit 24
```

### 结构化多轮问答 (使用 recall.py)
```bash
python3 "$MEMORY_SKILL_DIR/scripts/recall.py" recall \
  --query "<QUERY>" \
  --current-project
```
脚本按顺序创建 Session、发送 Turn、轮询直到完成或失败。后续追问复用返回的 `session_id`：
```bash
aiwc memory recall turn send <SESSION_ID> --query "<FOLLOW_UP>"
aiwc memory recall session get <SESSION_ID>
```
同一 Session 同时仅允许一个活动 Turn。取消使用：
```bash
aiwc memory recall turn cancel <TURN_ID>
```
只将 Recall 输出中的 `answer`、`sessionReferences`、`contentReferences` 和 `followUpSuggestions` 作为产品结果。

## 5. 故障排查 (Troubleshooting)

### 瞬时故障先重试，不要急着诊断

- `error returned from database: (code: 5) database is locked`：桌面版 AssetIWeave 正在写 `app.db`，属**瞬时占用**。
  直接重试同一命令（建议间隔 3~20 秒、最多 10 次左右），通常第 2~5 次即成功，**不要**因此改走其他命令或放弃取证。

### 环境诊断

仅在 CLI 报错、Engine 连通性异常或怀疑协议契约不匹配时，按需执行环境诊断：

```bash
MEMORY_SKILL_DIR="${ASSETIWEAVE_MEMORY_SKILL_DIR:-}"
if [ -z "$MEMORY_SKILL_DIR" ]; then
  for candidate in \
    "${CODEX_HOME:-$HOME/.codex}/skills/assetiweave-memory" \
    "$HOME/.claude/skills/assetiweave-memory" \
    "$HOME/.assetiweave/skills/.system/assetiweave-memory"; do
    if [ -f "$candidate/scripts/recall.py" ]; then MEMORY_SKILL_DIR="$candidate"; break; fi
  done
fi
python3 "$MEMORY_SKILL_DIR/scripts/recall.py" doctor
```

`doctor` 会单次聚合校验 Engine 协议契约与以下能力方法：
- `memory.recent.snapshot.get`
- `memory.context.resolve`
- `memory.project.get`
- `memory.recall.search`
- `memory.recall.session.create`
- `memory.recall.session.get`
- `memory.recall.turn.send`
- `memory.recall.turn.cancel`
