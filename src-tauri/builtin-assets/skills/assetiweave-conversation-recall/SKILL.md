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
   - `--type question|answer|tool|command|code|result`
   - `--timeline`

4. 如果首次搜索命中不足，改写关键词或分别搜索方法名、命令名、错误文本。不要仅因一次零命中就断言历史中不存在。

5. 对排名靠前且互不重复的 `question_id` 做渐进展开：

   ```bash
   assetiweave-cli conversation question get <question-id>
   ```

   对 `record-kind=web` 的命中，使用命中中的 `session_id` 读取网页记录，再定位相应 Question：

   ```bash
   assetiweave-cli conversation web-record get <session-id>
   ```

6. 只有在 Question 缺少前后决策、同一 Session 中存在关联问题，或用户明确要求完整回顾时，才读取整个 Session：

   ```bash
   assetiweave-cli conversation session get <session-id>
   ```

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
