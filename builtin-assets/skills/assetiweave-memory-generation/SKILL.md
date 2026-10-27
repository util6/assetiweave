---
name: assetiweave-memory-generation
description: AssetIWeave 记忆认知提炼工作流 SOP 与系统提示词（前置发现短路与节点 1 会话思维分叉提取）
---

# AssetIWeave Memory Generation SOP & Prompts

本 Skill 作为 AssetIWeave 认知记忆生成体系的标准执行指导（SOP）与系统提示词（System Prompt）模板，指导智能代理（Agent）从海量会话历史中提取高信号、可验证的思维分叉事实，并以 Markdown 优先格式持久化沉淀。

## 0. 全局认知提炼三节点架构概览 (Cognitive Distillation Architecture)

为了彻底根除传统机械启发式匹配带来的“复读用户长 Prompt、收录过程口语（如 ok/继续）、丢失真实分支”等认知漂移，AssetIWeave 采用分层解耦的三节点认知提炼流水线：

1. **Step 0 · 前置发现与 0-Token 水位短路 (Pre-discovery & 0-Token Watermark Short-circuit)**：
   扫描配置时间窗口（默认 48h/72h）内的活跃会话，通过轻量 CLI 获取会话更新时间戳并与磁盘已有 Markdown 文件的 Frontmatter `updated_at` 进行比对。**未变动会话实现 0-Token 秒级短路**，避免全量重跑与 Token 浪费。
2. **节点 1 · 单会话思维分叉与闭环提取 (Node 1: Session Branch & Closure Extraction)**（**本 Skill 核心**）：
   对发生变更或未处理的有信号会话，抽取轻量 QA 骨架，通过认知提炼生成该会话的思维分叉拓扑（显著区分 🍂 待探索悬空叶子节点与 ✅ 已完成闭环任务），原子落盘至 `sessions/<date>_<short_id>.md`。
3. **节点 2 · 近期研发脉络与项目叶子消融 (Node 2: Recent R&D Snapshot & Leaf Ablation)**：
   扫描近期所有单会话产物，生成近期全景快照 `recent/latest.md`，并将未闭环的叶子沉淀到对应项目 `projects/<slug>.md`。当后续会话对悬空叶子达成闭环时，自动执行消融转移。
4. **节点 3 · 全局经验与长期认知沉淀 (Node 3: Global Knowledge Consolidation)**：
   跨项目归纳稳定规则、架构约束与全局偏好，增量收敛至根目录 `MEMORY.md`。

---

## 1. Step 0 · 前置发现与 0-Token 水位短路 SOP

在调用任何大模型或执行昂贵文本处理前，**必须先执行 Step 0 水位检查**，确保只处理真正发生增量更新的会话。

### 步骤 1.1：获取近期活跃会话清单
调用 `aiwc` CLI 获取指定窗口期内的会话元数据（默认 `--window 48h`，可根据需要指定 `24h` 或 `72h`）：
```bash
aiwc --format json conversation list --window 48h
```

CLI 返回标准 JSON 信封数组：
```json
{
  "ok": true,
  "data": [
    {
      "short_id": "73c20273",
      "session_id": "conversation-session-73c20273fdf1bd1d08933a5dfe16f141170ecdb12dc6980ff25745b4cf202645",
      "updated_at": "2026-10-27T01:58:40Z",
      "turn_count": 3,
      "project_path": "/Users/util6/code-space/assetiweave",
      "project_name": "assetiweave"
    }
  ]
}
```

### 步骤 1.2：磁盘水位文件探查与短路判定
单会话记忆存储路径规范：
```
~/.assetiweave/memories/<tenant>/sessions/<date>_<short_id>.md
```
- `<tenant>`：默认租户为 `default`（若环境变量 `ASSETIWEAVE_MEMORY_ROOT` 存在，则以该根目录为准）。
- `<date>`：取自会话 `updated_at` 的日期部分（格式 `YYYY-MM-DD`）。
- `<short_id>`：8 位会话短 ID。

**比对与短路规则**：
1. 检查文件 `~/.assetiweave/memories/<tenant>/sessions/<date>_<short_id>.md` 是否存在；
2. 若文件存在，读取其开头的 YAML Frontmatter：
   ```yaml
   ---
   session_id: conversation-session-73c20273...
   updated_at: 2026-10-27T01:58:40Z
   ...
   ---
   ```
3. 比对 Frontmatter 中的 `updated_at` 与 `conversation list` 返回的 `updated_at`：
   - **时间戳严格一致**：说明会话自上次提炼后无新对话或变动，**直接 0-Token 跳过**，严禁重复提取！
   - **时间戳不一致或文件不存在**：标记为增量会话，进入节点 1 提炼流程。

---

## 2. 节点 1 · 单会话思维分叉与闭环提取 SOP

### 步骤 2.1：有界问答主干抽取 (QA Backbone Extraction)
对标记为增量的会话，通过 CLI 抽取高信噪比骨架，首选 `--qa` 模式过滤工具卡片与原始流水噪音：
```bash
aiwc conversation outline <short_id> --qa
```
*注：仅在极少数需要卡片类型或工具执行结果辅助判定的复杂排障会话中，才按需使用 `aiwc conversation outline <short_id> --full`。严禁全量 dump 原始会话全文字符串。*

`--qa` 返回紧凑的列式矩阵 JSON（`turns.columns = ["index", "turn_id", "question", "answer"]`），其中 `index` 为 **1-based** 轮次序号：
```json
{
  "ok": true,
  "data": {
    "session_id": "73c20273",
    "total_turns": 3,
    "turns": {
      "columns": ["index", "turn_id", "question", "answer"],
      "rows": [
        [1, "9633a9d7", "/implement-spec 落地[#135]...", null],
        [2, "faf95fe7", "/implement-spec 落地[#135]...", "# 任务完成报告..."]
      ]
    }
  }
}
```

### 步骤 2.2：认知提炼准则与四条红线 (Extraction Principles)

执行事实提炼时，必须严格恪守以下规范：

1. **❌ 严禁原样复刻用户长 Prompt 原文**：
   禁止无脑照抄用户的问题输入、附带的复制粘贴内容或冗长异常日志。必须概括提炼出实质技术意图与行动结论。
2. **❌ 严禁收录过程短语与交互噪音**：
   对话中的过程短语（如 `"ok"`、`"继续"`、`"1"`、`"好"`、`"确认"`、`"同意方案"`、`"按方案一办"`）仅为流程推进标记，严禁当作独立任务或分支收录。
3. **✅ 每一条提取项提炼为单行精炼结论，必须附带严格锚点**：
   每一条提炼项提炼为独立完整的单行客观陈述（通常 15~45 字），并必须严格附带 `[<short_id>#T<turn_index>](aiwc://session/<short_id>/turn/<turn_index>)` 协议锚点，其中 `turn_index` 严格对应 `turns.rows` 中的 1-based 轮次序号 `index`。
4. **两分法输出拓扑 (Branching Topology)**：
   必须严格区分为两类高信号条目：
   - `## 🍂 待探索的悬空叶子节点 (Unexplored Branches)`：对话中讨论了备选方案、后续改进点或待定方案，但本会话未实际展开、待后续探索的分支；若无，输出 `- 无待探索分支`。
   - `## ✅ 已完成的闭环任务 (Completed Tasks)`：本会话已彻底解决、编码实现完毕或经测试验证的闭环事项；若无，输出 `- 无已完成任务`。
5. **无信号会话判定 (Zero-Signal Detection)**：
   若会话全篇为闲聊、环境问候或未产生任何实质性技术方案/结论，两区块均输出无条目占位符，并在 Frontmatter 中标记 `has_signal: false`。

---

## 3. 节点 1 系统提示词模板 (Node 1 System Prompt Template)

当调用外部 LLM 或通过子代理执行节点 1 提炼时，使用如下标准 System Prompt：

```text
你是一个专业的高信号技术记忆架构师。你的唯一职责是从给定的会话 QA 骨架中提取高信噪比的“思维分叉与闭环事实”。

【会话上下文】
- session_id: {session_id}
- short_id: {short_id}
- project_path: {project_path}
- updated_at: {updated_at}
- turn_count: {turn_count}

【输入数据】
以下为会话各轮次的问答骨架（包含 index 轮次序号、turn_id、用户提问 question、模型回答 answer）：
{turns_json}

【提炼准则】
1. 区分两类条目：
   - 🍂 待探索的悬空叶子节点：会话中探讨过但未展开、提出的备选路径、后续优化方向或待决技术项；
   - ✅ 已完成的闭环任务：已编码完成、架构方案确认落地、缺陷修复完毕或经验证通过的事项。
2. 严禁原样复刻用户长 Prompt 原文或冗长错误日志，必须提炼概括为单行客观技术事实。
3. 严禁收录 "ok"、"继续"、"1"、"好"、"确认" 等过程交互短语。
4. 每一条提取项提炼为单行精炼结论，必须严格附带锚点格式：
   - [{short_id}#T{turn_index}](aiwc://session/{short_id}/turn/{turn_index}): <精炼结论>
   （turn_index 必须严格对应轮次 1-based 序号）。
5. 若无实质进展或全为闲聊，has_signal 标为 false，两区块分别输出 "- 无待探索分支" 与 "- 无已完成任务"。

【输出格式规范】
必须严格输出以下格式的 Markdown 内容，包含 YAML Frontmatter 与两个核心区块，严禁输出任何额外闲聊：

---
session_id: {session_id}
short_id: {short_id}
project_path: {project_path}
updated_at: {updated_at}
turn_count: {turn_count}
has_signal: true|false
---

# 会话 {short_id} 思维分叉与任务进展

## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
- [{short_id}#T<turn_index>](aiwc://session/{short_id}/turn/<turn_index>): <精炼待探索分支>

## ✅ 已完成的闭环任务 (Completed Tasks)
- [{short_id}#T<turn_index>](aiwc://session/{short_id}/turn/<turn_index>): <精炼已闭环事实>
```

---

## 4. Few-shot 对比案例库 (Positive vs. Negative Examples)

### 案例 1：用户长指令复刻 vs. 语义闭环提炼
- ❌ **机械复读（劣质反例）**：
  `- [73c20273#T1](aiwc://session/73c20273/turn/1): /implement-spec 落地[#135](https://github.com/util6/assetiweave/issues/135) 极其所有子工单`
  *(缺陷：机械照抄用户命令输入，毫无提炼，丢失最终回答中的实际闭环事实)*
- ✅ **认知提炼（标准正例）**：
  `- [73c20273#T2](aiwc://session/73c20273/turn/2): 落地 ADR-0018 Markdown 优先架构并完成全量单元与契约测试 (#135)`

### 案例 2：过程短语噪音 vs. 过滤忽略
- ❌ **机械复读（劣质反例）**：
  `- [098b0c4b#T14](aiwc://session/098b0c4b/turn/14): 完成了本轮目标: ok`
  *(缺陷：将用户的日常口语短回复当作完成任务收录，产生垃圾节点)*
- ✅ **认知提炼（标准正例）**：
  *(直接过滤忽略 T14，不产生无价值条目)*

### 案例 3：底层错误信息 vs. 故障排查事实
- ❌ **机械复读（劣质反例）**：
  `- [014c6b32#T8](aiwc://session/014c6b32/turn/8): HTTP 401: Your authentication token has been invalidated. Please try signing in again.`
  *(缺陷：直接把 HTTP 报错文本丢入记忆库)*
- ✅ **认知提炼（标准正例）**：
  `- [014c6b32#T8](aiwc://session/014c6b32/turn/8): 诊断 OpenClaw 本地代理网关 Token 失效并明确需重新登录鉴权`

### 案例 4：口语方案讨论 vs. 待探索悬空分支
- ❌ **机械复读（劣质反例）**：
  `- [d3fc3bcd#T2](aiwc://session/d3fc3bcd/turn/2): 在问答中提出的备选分支或延伸方案: 那就全面脱钩Go的CLI，包括CI和默认编译安装等行为...`
  *(缺陷：简单通过关键词启发式拼接用户口语长句)*
- ✅ **认知提炼（标准正例）**：
  `- [d3fc3bcd#T2](aiwc://session/d3fc3bcd/turn/2): 全面脱钩旧版 Go CLI 构建链与 CI 依赖，仅保留归档源码作为历史留存`

---

## 5. 产物规范与磁盘原子写入协议

提取完成后，产物必须严格遵循原子写入与安全规范：

1. **落盘路径**：
   `~/.assetiweave/memories/<tenant>/sessions/<date>_<short_id>.md`
2. **原子写入流程**：
   - 在目标父目录内先创建临时文件 `.tmp_<uuid>.md`；
   - 写入脱敏后的 Markdown 正文；
   - 执行原子重命名（`fs::rename`）覆盖目标文件，确保在断电或并发读时不会发生损坏。
3. **敏感信息脱敏 (Redaction)**：
   自动检测并脱敏 API Key、Bearer Token、私钥与密码凭据，替换为 `[REDACTED]`。
4. **高信噪比体积基线**：
   单会话提取结果必须保持精炼紧凑，单文件体积严格控制在 **1KB ~ 3KB**（50 行以内），禁止输出数万字完整对话流水。
