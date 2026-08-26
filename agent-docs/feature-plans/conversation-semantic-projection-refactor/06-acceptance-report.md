# Conversation 语义投影重构：验收记录

日期：2026-08-26

复审状态：**未完成最终验收**

详细审计：[`07-post-luna-audit.md`](./07-post-luna-audit.md)

## 已由当前事实证明的范围

- `conversation_questions` 与 `web_record_questions` 均只保留
  `tenant_id`、`id`、`session_id`、`title`、`created_at`、`updated_at`。
- `conversation_question_turns` / `web_record_question_turns` 是 Question–Turn membership
  的持久化 Authority；真实数据库没有重复、跨 Session 或孤立 membership。
- 新 Adapter fixture 证明一次 Shell Execution 可保存为一个 Part，并在读取时投影多个
  Content Node；前端短 ID 使用 Part 片段加 `:nN` 节点判别符。
- Search、Export、公开 Question Detail、Engine contract 和前端生产读取已经使用
  Content Node locator；Question 表不再保存正文快照。
- 启动崩溃已修复：已发布 migration 恢复原始字节，只对已知错误 checksum 和已验证
  contract 形状执行一次性修复，后续补偿由新 migration 完成。
- `conversation.data.repair` 非 dry-run 执行前强制创建并校验 SQLite 备份；调用方不再能
  关闭备份。

## 仍阻止最终验收的事实

- 真实数据库仍有 `44,962` 个历史 Shell execution 分组由多个 Part 保存。缺少可靠来源时
  禁止自动拼接；需要按可证明来源分批 full resync。
- Luna 修改后的 `202608250005` 曾先删除 Question 快照再补审计，导致 `6,131` 行只能保留
  保守依赖计数，不能重建迁移前逐字段差异。
- 后台“取消”只改变 TaskRuntime 状态，worker 和搜索重建没有消费取消 token；不能据此
  宣称协作式取消成立。
- 真实数据修复总耗时 `547,978 ms`，其中 185,579 文档搜索索引重建耗时 `525,986 ms`；
  现有验收没有大会话性能 fixture，也没有内存峰值和响应性证据。
- 前端没有普通用户可触发 audit/repair 的入口，CLI 只有通用 `api call`，没有专用命令。
- `ConversationContentCards` 仍保留 `parts`/seed fallback 与可选 `nodes`；后端内部仍有
  Card 命名的投影 seam。生产读取已走 Content Node，但 #15 的兼容收缩没有完全结束。

## 当前质量门禁

| 门禁 | 复审结果 |
|---|---|
| `pnpm typecheck` | PASS |
| `pnpm test` | PASS：110 files / 559 tests；首次高系统负载运行出现超时，原命令复跑通过 |
| `pnpm build` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace` | PASS：698 passed / 1 ignored |
| `go vet -C cli ./...` | PASS |
| `go test -C cli -race ./...` | PASS |
| 临时 DB `pnpm cli:contract` | PASS，生成物已更新 |
| 临时 DB `pnpm cli:test:e2e` | PASS |
| `conversation-adapters:check/test` | PASS：7 packages / 54 tests |
| fresh DB 与真实 DB startup self-check | PASS |
| `pnpm tauri:dev` 真实 DB 启动 | PASS：进入 `target/debug/assetiweave`，无新增 panic |

## 范围裁决

Memory 大模块后续整体重写。本轮不审计、不修复、不验收 Memory 功能；任何既有 Memory
测试通过只表示仓库回归门禁未被当前补丁破坏，不构成该模块的功能结论。
