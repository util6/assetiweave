---
name: assetiweave-memory
description: 使用 AssetIWeave 的渐进式 Memory 回答“上次怎么做、为什么这样决定、今天继续什么”，或对明确项目/来源/Session 范围做完整整理。区分轻量 Dream、精准回忆和完整整理；默认取得带 Session/Question/Block 标识的本地 evidence bundle 供当前宿主 Agent 综合，只有用户明确要求应用调用外部 AI 时才启用双阶段综合。
---

# AssetIWeave Memory

把 `assetiweave-cli memory` 作为唯一业务入口。不要直接查询 SQLite、第三方 Session 数据库或复制 Memory 规则。

## 选择流程

1. 用户问“今天继续什么”或近期进展时，读取确定性 Overview：

   ```bash
   assetiweave-cli memory overview --current-project
   ```

   需要近期自动摘要时再查看 `memory dream status|list|get`。Dream Note 只作线索和路由提示；事实回答必须回到原始 Conversation evidence。

2. 用户问具体历史问题时，先运行精准回忆。默认不要加 `--ai`：

   ```bash
   assetiweave-cli memory recall run \
     --query "<用户问题>" \
     --current-project \
     --format compact-json
   ```

   用返回的 evidence bundle 在当前宿主中综合回答。保留每条采用证据的 `record_kind`、`session_id`、`question_id`、`block_id` 和 evidence ID。

3. 用户明确要求完整复盘或整理时，必须指定 App、Source、Project 或 Session 范围。先预览覆盖，再分批执行：

   ```bash
   assetiweave-cli memory recall preview --full --current-project
   assetiweave-cli memory recall run --full --current-project --format compact-json
   ```

   披露 `total_question_count`、本批覆盖、跳过、失败、检索后端和截断状态。不要把分页或预算截断描述为“全部完成”。

4. 只有用户明确同意由 AssetIWeave 启动配置的 OpenCode/Gemini 外部进程时，才给 Recall 加 `--ai`，或运行 Dream：

   ```bash
   assetiweave-cli memory recall run --query "<问题>" --current-project --ai
   assetiweave-cli memory dream run --current-project
   ```

## 证据与审核边界

- 把 Conversation Card 当作事实证据；把 Dream 和 extraction 当作派生材料。
- 区分证据直接支持的事实、跨证据推断、冲突与证据不足。命令和代码只能来自相应 Card。
- 来源不可用时使用快照解释，并标注 `source_unavailable`；不要假装已经重新验证。
- AI 综合只生成 candidate。正式 Memory 只能由用户手工创建，或通过 `memory candidate accept` 审核接受。
- 用户要保存结论时，先展示 candidate 的类型、内容和 evidence，再接受或拒绝；不要自动 supersede、归档或删除旧 Memory。
- 需要原始历史的更细粒度二次检索时，使用 `assetiweave-conversation-recall`，不要让 Dream 代替 Card 检索。
