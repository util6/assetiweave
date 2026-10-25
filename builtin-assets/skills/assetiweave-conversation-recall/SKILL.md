---
name: assetiweave-conversation-recall
description: 从 AssetIWeave 的标准化对话 Card 中渐进检索历史证据并回答回忆问题。用户询问“上次怎么处理”“某个对话用了什么方法、命令或代码”“以前关于某项目讨论过什么”，或需要跨 Codex、Claude、OpenCode、网页记录定位原始对话时使用。
---

# Conversation Recall

把 AssetIWeave CLI 当作唯一检索入口。先读取少量搜索命中，再按需展开 Question 或 Session；不要一开始加载完整对话历史，也不要直接查询 SQLite 或第三方 Session 数据库。

## 检索流程

1. 确认 CLI 与检索契约：

   ```bash
   assetiweave-cli version
   assetiweave-cli schema conversation.search
   ```

2. 从用户问题提取主题、项目、时间范围、记录类型和 Card 类型。用户指向当前项目时优先使用 `--current-project`。

3. 先搜索紧凑证据：

   ```bash
   assetiweave-cli conversation search --query "<关键词>" --current-project --format compact-json --limit 20
   ```

   按需要添加：

   - `--record-kind session|web`
   - `--since <YYYY-MM-DD>` 与 `--until <YYYY-MM-DD>`
   - `--kind question|answer|reasoning|tool|command|code|result`
   - `--kind <adapter.namespaced-card-kind>` 用于适配器专属 Card
   - `--timeline`

4. 如果首次搜索命中不足，改写关键词或分别搜索方法名、命令名、错误文本。不要仅因一次零命中就断言历史中不存在。

5. 对命中的目标会话或轮次，先读取轻量结构骨架：

   ```bash
   aiwc conversation outline <id>
   ```

   `outline` 支持 Session ID、Turn ID 或 Card ID（支持 8 字符短 ID），返回列式 Matrix JSON 紧凑骨架。先读 `question` 意图与 `activities` 连续去重摘要。

6. 仅在确实需要代码 diff、错误堆栈、配置参数或模型精确回复细节时，依据卡片 ID 单点下钻：

   ```bash
   aiwc conversation card get <card-id>
   ```

   加 `--raw` 开关可直接将原始文本输出到 stdout，便于代码/日志提取。**严禁全量 dump 会话正文**，每次仅查 1~2 张关键卡片，查完即止。

## 同步边界

默认检索已有数据。只有用户要求最新记录，或结果明显缺少近期 Session 时，才先预览并同步：

```bash
assetiweave-cli conversation sync --dry-run
assetiweave-cli conversation sync
```

网页采集脚本失效、认证过期或环境缺失时，不在本 Skill 内猜测修复；改用 `assetiweave-web-conversation-repair`。

## 证据与回答

- 为采用的证据保留 `session_id`、`question_id`、`block_id`、Card 类型和事件时间。
- 区分原始记录、基于多条记录的推断和仍不确定的内容。
- 优先回答方法、原因、结果和可复用步骤；命令或代码必须来自相应 Card，不能凭印象补全。
- 多个 Session 结论冲突时，展示时间顺序并说明后续记录是否替代早期方案。
- 搜索发生分页、时间过滤或来源缺失时，明确说明检索范围，不声称结果穷尽全部历史。
