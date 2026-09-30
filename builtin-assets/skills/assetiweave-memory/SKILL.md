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
     aiwc conversation session outline <short-id-or-any-id>
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
     aiwc conversation session outline <short-id>
     ```
   - **严禁二手摘要交差 (No Second-hand Summary)**：
     对于任何涉及技术方案、Bug 修复细节、关键配置或历史讨论的提问，**严禁仅凭 `context resolve` 的 3 行粗略摘要草草回复**！必须在第 1 轮对话内完成取证核实，将原始验证细节连同 **Session 短 ID** 一同交付给用户！

### 4 级逐级取证梯级 (4-Tier Progressive Evidence Protocol)
1. **Tier 0: 会话骨架还原 (Session Outline)**
   ```bash
   aiwc conversation session outline <any-id>
   ```
   获取会话的紧凑骨架树。
   - 连续相同种类的卡片（如多张连续的 tool_call、tool_result 或 answer）已被自动折叠合并，输出结构包含 `card_kind`、`count` 与 `card_ids`。
   - 骨架中**仅对用户提问（question / user prompt）保留完整提问文本**，其余卡片一律不含具体内容，极致压低上下文 Token 消耗。

2. **Tier 1: 用户提问意图透视 (Question Intent Scan)**
   - 优先自顶向下遍历 Tier 0 骨架中包含的各轮用户提问（Prompt）文本。
   - 在绝大多数事实追溯、技术问答和上下文还原场景中，仅凭用户的问题轨迹即可明确该会话在何时探讨了何种方案、排查了什么问题，直接形成可信判断。

3. **Tier 2: 判定门禁 (Decision Gate)**
   在进入下一步之前，必须执行严格的门禁判断：
   - **推断充足**：若根据用户的提问和骨架已足以回答用户问题，**立即输出结论，严禁多余下钻取证**！
   - **事实缺失**：仅当必须获取具体的代码 diff、错误堆栈、配置参数或模型精确回复细节时，才允许进入 Tier 3。

4. **Tier 3: 单点卡片按需下钻 (Point Drilldown on Card IDs)**
   - 仅针对 Tier 2 判定必须阅读的极少数特定卡片，依据其 `card_id`（支持 8 字符短 ID）精准单点拉取：
     ```bash
     aiwc conversation block get <card-id>
     ```
   - **严禁全量 dump 会话正文**，每次仅查 1~2 张关键卡片，查完即止，按需取证。

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
