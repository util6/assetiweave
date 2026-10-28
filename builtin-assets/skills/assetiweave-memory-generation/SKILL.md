---
name: assetiweave-memory-generation
description: AssetIWeave 记忆认知提炼工作流 SOP 与系统提示词（前置发现短路、节点 1 思维分叉提取、节点 2 Recent 聚合与 Project 叶子语义消融、节点 3 Global 长期记忆沉淀）
---

# AssetIWeave Memory Generation SOP & Prompts

本 Skill 作为 AssetIWeave 认知记忆生成体系的标准执行指导（SOP）与系统提示词（System Prompt）模板，指导智能代理（Agent）从海量会话历史中提取高信号、可验证的思维分叉事实，并以 Markdown 优先格式持久化沉淀。

## 0. 全局认知提炼三节点架构概览 (Cognitive Distillation Architecture)

为了彻底根除传统机械启发式匹配带来的“复读用户长 Prompt、收录过程口语（如 ok/继续）、丢失真实分支”等认知漂移，AssetIWeave 采用分层解耦的三节点认知提炼流水线：

1. **Step 0 · 前置发现与 0-Token 水位短路 (Pre-discovery & 0-Token Watermark Short-circuit)**：
   扫描配置时间窗口（默认 48h/72h）内的活跃会话，通过轻量 CLI 获取会话更新时间戳并与磁盘已有 Markdown 文件的 Frontmatter `updated_at` 进行比对。**未变动会话实现 0-Token 秒级短路**，避免全量重跑与 Token 浪费。
2. **节点 1 · 单会话思维分叉与闭环提取 (Node 1: Session Branch & Closure Extraction)**：
   对发生变更或未处理的有信号会话，抽取轻量 QA 骨架，通过认知提炼生成该会话的思维分叉拓扑（显著区分 🍂 待探索悬空叶子节点与 ✅ 已完成闭环任务），原子落盘至 `sessions/<date>_<short_id>.md`。
3. **节点 2 · 近期研发脉络与项目叶子消融 (Node 2: Recent R&D Snapshot & Leaf Ablation)**：
   扫描近期所有单会话产物，生成近期全景快照 `recent/latest.md`（及日期归档 `recent/<YYYY-MM-DD>.md`）。同时读取各项目现存的 `projects/<slug>.md`，由 Agent 执行**语义级消融**：若旧悬空叶子已被本轮解决或因方向变更废弃，坚决剔除；若有新增叶子则去重合入；未触及项原样保留，并原子写回。
4. **节点 3 · 全局经验与长期认知沉淀 (Node 3: Global Knowledge Consolidation)**：
   跨项目审视近期研发脉络与架构决议，仅当存在跨项目通用、长期稳定的工程准则、架构偏好或避坑铁律时增量更新 `MEMORY.md`；严禁写入单项目业务流水。

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

## 4. 节点 1 Few-shot 对比案例库 (Node 1 Positive vs. Negative Examples)

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

## 5. 节点 2 · Recent 窗口聚合与 Project 悬空叶子语义消融 SOP

节点 2 负责将窗口期内的单会话产物汇聚为全局研发脉络快照，并针对各项目既有的悬空叶子池进行**大模型语义级消融与增量演进**。

### 步骤 5.1：有信号会话聚合与 Recent 快照生成
1. **聚合范围**：严格仅聚合当前近期窗口内（Step 0 发现的有信号会话 `sessions/<date>_<short_id>.md`），无信号会话（`has_signal: false`）直接略过。
2. **分组输出**：按 `project_path`（或归一化后的项目标示）对有信号会话进行分组，汇集该项目在近期窗口内的所有悬空叶子与闭环任务。
3. **生成 Recent 快照**：
   - 输出到 `recent/latest.md`（始终反映最新研发脉络）；
   - 同步原子快照归档到 `recent/<YYYY-MM-DD>.md`（以当前日期命名）。
   - 格式规范：
     ```markdown
     # 近期研发进展与待探索分支 (Recent Work & Unexplored Branches)

     ## 项目: /Users/util6/code-space/assetiweave

     ### 🍂 待探索的悬空叶子节点 (Unexplored Branches)
     - [73c20273#T1](aiwc://session/73c20273/turn/1): 探索 SQLite WAL 模式高并发评测
     - [s2#T2](aiwc://session/s2/turn/2): 评估 Tantivy 索引剪枝机制

     ### ✅ 已完成的闭环任务 (Completed Tasks)
     - [73c20273#T2](aiwc://session/73c20273/turn/2): 落地 ADR-0018 Markdown 优先架构并完成全量测试
     ```

### 步骤 5.2：项目现有悬空叶子读取与语义消融决策 (Semantic Leaf Ablation)
传统的文本字符串精确匹配无法识别“同一个方案在后续会话中用不同词汇被实现并验证”。因此，悬空叶子的消融必须由 Agent 基于技术语义进行推理判定：

1. **读取现有池**：
   读取目标项目的现有记忆文件 `projects/<slug>.md`（如 `projects/assetiweave.md`），提取其中的 `existing_leaves`。
2. **收集本轮增量**：
   从本轮 Recent 聚合中获取该项目的 `completed_tasks`（已闭环任务）与 `new_leaves`（本轮新增待探索分支）。
3. **大模型语义消融规则**：
   - **规则 A（闭环即消融）**：若旧悬空叶子中的设想、优化或探索目标已经在 `completed_tasks` 中被编码解决、落地实现或验证通过，坚决将其从叶子池中消融剔除。
   - **规则 B（路线变更即废弃）**：若旧悬空叶子因最新技术决策、架构重构被明确否定、废弃或替代（例如旧叶子提议维护旧格式兼容，但最新决策已确立彻底脱钩），坚决将其从叶子池中剔除。
   - **规则 C（增量悬空叶子合入）**：将 `new_leaves` 中未在池中、且未被本轮闭环的事项加入叶子池。
   - **规则 D（未决历史叶子原样保留）**：本轮未被触及、未被解决的既有悬空叶子，原样保留在池中。
   - **规则 E（锚点与单行完整性）**：所有保留与新增的叶子必须严格保持 `- [<short_id>#T<turn>](aiwc://...): <精炼陈述>` 单行格式，严禁自由多行展开。

### 步骤 5.3：原子写回 `projects/<slug>.md`
将消融并合入后的最终叶子集合，按规范原子写回对应项目文件：
```markdown
---
project_path: /Users/util6/code-space/assetiweave
updated_at: 2026-10-27T07:20:00Z
---

# 项目记忆: /Users/util6/code-space/assetiweave

## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
- [73c20273#T1](aiwc://session/73c20273/turn/1): 探索 SQLite WAL 模式高并发评测
- [old1#T3](aiwc://session/old1/turn/3): 调研跨平台文件监视器 Notify 深度集成
```

---

## 6. 节点 2 系统提示词模板 (Node 2 System Prompt Template)

当调用外部 LLM 或子代理执行项目悬空叶子池语义消融时，使用如下标准 System Prompt：

```text
你是一个精通技术演化与代码库生命周期的记忆架构师。你的唯一职责是对目标项目的“待探索悬空叶子池”执行语义级消融（Leaf Ablation）与增量合入。

【项目上下文】
- project_path: {project_path}
- slug: {slug}

【输入数据】
1. 现有悬空叶子池 (Existing Leaves):
{existing_leaves_markdown}

2. 本轮已完成闭环任务 (Completed Tasks):
{completed_tasks_markdown}

3. 本轮新增待探索分支 (New Leaves):
{new_leaves_markdown}

【消融推理与更新准则】
1. 闭环消融：逐项对比 Existing Leaves 与 Completed Tasks。若 Existing Leaf 所探讨的设想、优化或方案已经在某项 Completed Task 中被落地、解决或验证，坚决从池中消融（剔除）。
2. 路线废弃：若 Existing Leaf 因本轮完成的架构决策被明确否定、替代或废弃（例如重构全面脱钩旧组件），坚决从池中消融（剔除）。
3. 历史未决保留：凡本轮未被解决、未被否定的既有叶子，必须严格原样保留，严禁丢失历史线索。
4. 新增叶子合入：将 New Leaves 中尚未闭环的新技术分支合入池中，去除与既有条目语义重复的项。
5. 严格格式红线：
   - 每一条保留或新增的叶子必须为单行列表项；
   - 必须保留原始锚点前缀格式：- [<short_id>#T<turn>](aiwc://session/<short_id>/turn/<turn>): <精炼描述>
   - 严禁添加二级说明、解释性子列表（如 "  - 说明:"）或自由段落；
   - 若池中最终没有任何待探索分支，输出 "- 暂无待探索分支"。

【输出格式规范】
严格输出更新后的 Markdown 内容，包含 YAML Frontmatter 与唯一的分支区块，严禁任何额外闲聊：

---
project_path: {project_path}
updated_at: {current_timestamp}
---

# 项目记忆: {project_path}

## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
- [<short_id>#T<turn>](aiwc://session/<short_id>/turn/<turn>): <精炼待探索分支>
```

---

## 7. 节点 2 Few-shot 语义消融对比案例库 (Node 2 Positive vs. Negative Ablation Examples)

### 案例背景
- **项目路径**：`/Users/util6/code-space/assetiweave`
- **现有叶子池 (Existing Leaves)**：
  ```markdown
  - [s1#T1](aiwc://session/s1/turn/1): 探索在应用层引入 Redis 缓存加速资产元数据检索
  - [s1#T2](aiwc://session/s1/turn/2): 评测 gRPC vs IPC 在跨进程通信中的性能损耗
  - [s1#T3](aiwc://session/s1/turn/3): 探讨保留旧版 Go CLI 兼容层以支持历史脚本
  ```
- **本轮已完成闭环任务 (Completed Tasks)**：
  ```markdown
  - [s2#T1](aiwc://session/s2/turn/1): 落地 Redis 缓存层并完成基准测试验证 (#140)
  - [s2#T2](aiwc://session/s2/turn/2): 决议全面脱钩旧版 Go CLI，迁移至 Rust 原生命令行并归档旧代码 (#141)
  ```
- **本轮新增待探索分支 (New Leaves)**：
  ```markdown
  - [s2#T3](aiwc://session/s2/turn/3): 评估 SQLite WAL 模式在高并发批量挂载下的性能表现
  ```

### 对比结果
- ❌ **机械未消融（劣质反例）**：
  ```markdown
  ## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
  - [s1#T1](aiwc://session/s1/turn/1): 探索在应用层引入 Redis 缓存加速资产元数据检索
  - [s1#T2](aiwc://session/s1/turn/2): 评测 gRPC vs IPC 在跨进程通信中的性能损耗
  - [s1#T3](aiwc://session/s1/turn/3): 探讨保留旧版 Go CLI 兼容层以支持历史脚本
  - [s2#T3](aiwc://session/s2/turn/3): 评估 SQLite WAL 模式在高并发批量挂载下的性能表现
  ```
  *(缺陷：机械追加，Redis 缓存明明已由 s2#T1 编码完成并落地验证，Go CLI 兼容明明已被 s2#T2 决议脱钩废弃，却依然死锁在待探索池中，导致叶子池无限膨胀、信噪比迅速崩溃)*

- ✅ **语义消融标准产物（标准正例）**：
  ```markdown
  ---
  project_path: /Users/util6/code-space/assetiweave
  updated_at: 2026-10-27T07:20:00Z
  ---

  # 项目记忆: /Users/util6/code-space/assetiweave

  ## 🍂 待探索的悬空叶子节点 (Unexplored Branches)
  - [s1#T2](aiwc://session/s1/turn/2): 评测 gRPC vs IPC 在跨进程通信中的性能损耗
  - [s2#T3](aiwc://session/s2/turn/3): 评估 SQLite WAL 模式在高并发批量挂载下的性能表现
  ```
  *(消融分析：*
  *1. `s1#T1`（Redis 缓存探索）与 `s2#T1`（落地 Redis 缓存层）语义闭环，**成功消融剔除**；*
  *2. `s1#T3`（保留 Go CLI 兼容）与 `s2#T2`（决议全面脱钩 Go CLI）路线冲突且废弃，**成功剔除**；*
  *3. `s1#T2`（gRPC vs IPC）本轮未触及，**原样保留**；*
  *4. `s2#T3`（SQLite WAL 评估）为新提出分支，**去重合入**。*
  *叶子池维持极其精炼、高度可操作的健康水位。)*

---

## 8. 节点 3 · Global MEMORY.md 长期记忆沉淀 SOP

节点 3 作为全局长期认知的顶层过滤器，负责跨越单一项目的边界，沉淀普遍适用、长期稳定的通用架构法则与工程准则。

### 步骤 8.1：长期记忆筛选铁律 (Global Memory Criteria)

Global `MEMORY.md` 是所有 Agent 跨项目工作的最高共享认知，必须极其克制，防止沦为琐碎任务的垃圾桶。

1. **必须准入准则 (Whitelisted)**：
   - **跨项目通用的工程规范**：例如语言特定规范（如 Rust 业务代码行数硬限制、测试拆分同级镜像模式）。
   - **系统运行时核心铁律**：例如执行脚本严禁内联 `python3 -c` 多行代码以防挂死、必须落地临时文件。
   - **不可动摇的架构原则**：例如本地优先（Local-first）、单层直接软链接、数据变更必须依赖迁移脚本。
   - **重大高频避坑经验**：经反复验证确认的深坑解决方案与安全隔离红线。
2. **坚决禁止准入准则 (Strictly Prohibited)**：
   - ❌ **单项目业务功能进展**：如“完成了 assetiweave 的技能扫描优化”。
   - ❌ **局部缺陷排查流水**：如“修复了某函数的空指针异常”。
   - ❌ **瞬态测试或部署数据**：如“本轮运行通过了 863 个单元测试”。
   - ❌ **临时待办或特定 Issue 关联**：特定 Issue 的跟踪收敛于项目悬空叶子池，严禁污染 Global Memory。

### 步骤 8.2：增量更新与去重写入
1. 审视近期研发全景 `recent/latest.md` 以及各项目最新决策；
2. 读取现有 `MEMORY.md`；
3. 判定是否存在值得沉淀为全局工程准则的规则；
4. 若存在，以增量方式精炼合并入 `MEMORY.md` 对应区块，并执行原子写盘。若无增量全局准则，保持不变。

---

## 9. 节点 3 系统提示词模板 (Node 3 System Prompt Template)

当调用外部 LLM 或子代理执行全局长期记忆提炼时，使用如下标准 System Prompt：

```text
你是一个最高层级的系统架构与工程规范委员会主席。你的唯一职责是从近期的技术研发事实中，提取出“跨项目通用、长久稳定、最高等级的工程准则与架构避坑铁律”，并沉淀到全局长期记忆 MEMORY.md。

【输入数据】
1. 现有的全局记忆 (Existing Global Memory):
{existing_memory_markdown}

2. 近期研发全景事实 (Recent R&D Facts):
{recent_facts_markdown}

【沉淀准则】
1. 极致克制：只有具备“跨项目通用价值”、“长期稳定有效”且“违背将引发严重故障或风格失控”的技术铁律才能写入。
2. 绝对禁止写入：
   - 任何特定项目的业务功能描述（如“修复了某个 UI 按钮样式”）；
   - 任何单次调试或临时异常日志；
   - 任何瞬态指标（如版本号变更、测试用例统计）。
3. 格式规范：
   - 按主题分组（如：通用工程规范、架构红线、脚本与环境避坑铁律）；
   - 每一条必须为陈述明确、可严格执行的单行准则，文字精炼（20~50 字）；
   - 保留既有已沉淀的稳定规则，仅增量合并新发掘的通用铁律。

【输出格式规范】
严格输出更新后的 Markdown 内容，严禁任何额外闲聊：

# 全局长期记忆与工程准则 (Global Memory)

## 📐 通用工程规范与架构准则
- <精炼准则 1>
- <精炼准则 2>

## ⚠️ 运行时安全与避坑铁律
- <精炼铁律 1>
- <精炼铁律 2>
```

---

## 10. 节点 3 判定案例与决策矩阵 (Global Memory Decision Cases)

| 事实候选条目 | 判定结果 | 决策理由与归宿 |
| :--- | :--- | :--- |
| **执行脚本时严禁内联 `python3 -c "..."` 多行代码，必须写入临时文件再执行以防进程卡死** | ✅ **准入 Global** | **跨语言、跨项目通用的硬性环境避坑铁律**，所有 Agent 均必须遵守。 |
| **Rust 单业务文件严格控制在 500 行以内，测试拆分至同级独立的 `*_tests.rs` 镜像文件** | ✅ **准入 Global** | **长期稳定的全局代码组织与架构约束**，具备跨项目指导价值。 |
| **本地优先与 Markdown 优先：所有业务记忆均落盘至本地文件，SQLite 仅作为轻量水位** | ✅ **准入 Global** | **系统级核心架构原则**，定义了全仓的设计基调。 |
| **在 `memory_workflow_recent_project.rs` 中加固了 `parse_session_markdown` 列表解析** | ❌ **拦截** | **单项目、局部代码改动流水**。应保留在对应会话与项目闭环任务中，严禁污染 Global。 |
| **全部 863 个单元测试通过，耗时 316 秒** | ❌ **拦截** | **瞬态指标与测试日志**，毫无长期规则沉淀价值。 |
| **为特定客户修复了导出 PDF 时的中文乱码问题** | ❌ **拦截** | **业务特定需求缺陷**，属于项目历史记录，非全局架构规则。 |

---

## 11. 产物规范与磁盘原子写入协议

提取与消融完成后，所有层次的产物必须严格遵循存储路径与原子写入规范：

1. **磁盘路径映射矩阵**：
   - 单会话分叉：`~/.assetiweave/memories/<tenant>/sessions/<date>_<short_id>.md`
   - 近期研发快照：`~/.assetiweave/memories/<tenant>/recent/latest.md` 与 `recent/<date>.md`
   - 项目悬空叶子池：`~/.assetiweave/memories/<tenant>/projects/<slug>.md`
   - 全局长期记忆：`~/.assetiweave/memories/<tenant>/MEMORY.md`
2. **原子写入协议 (Atomic Protocol)**：
   - 在目标父目录内先创建临时文件 `.tmp_<uuid>.md`；
   - 写入脱敏后的 Markdown 正文；
   - 调用原子重命名（如 `std::fs::rename`）覆盖目标文件，确保在进程崩溃或并发读取时零损坏。
3. **敏感信息脱敏 (Redaction)**：
   自动检测并脱敏 API Key、Bearer Token、私钥与密码凭据，统一替换为 `[REDACTED]`。
4. **高信噪比与文件体积基线**：
   - 单会话文件：控制在 **1KB ~ 3KB**（50 行以内）；
   - 项目悬空叶子池：通过动态语义消融始终将未决分支收敛在 **20 项以内**，严禁无限蔓延；
   - 全局长期记忆：精炼扼要，严格控制在 **100 行以内**。
