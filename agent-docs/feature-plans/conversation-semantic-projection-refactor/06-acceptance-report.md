# Conversation 语义投影重构：验收记录

日期：2026-08-26

## 实现范围

- #12 前端使用 `Session → Question → Turn → Part → Content Node` 唯一读取模型。
- #13 增加审计表、dry-run/backup/apply/verify/rollback、可选可靠来源全量重同步和后台
  TaskRuntime 进度/事件/轮询补偿。
- #14 通过受控 SQLite 表重建移除 Question 的六个旧字段；顺序从 membership 与 Turn 推导。
- #15 删除公开 Card DTO、旧内容节点数组和 UI execution 的 `command` 兼容旁路；CLI contract
  由 `pnpm cli:contract` 生成。
- Memory 功能性重写不在本轮范围；没有新增 Memory 审计、证据重算或行为设计。

## 行为证据

- 临时 SQLite 集成测试覆盖孤立 Part/membership 的 audit、dry-run、apply、重复 apply、
  failed resync 保留事实，以及备份 rollback。
- Rust projection 测试证明一个 Codex Shell Part 可以生成多个稳定 Content Node，而 Part
  数量不随展示节点数量增加。
- BackgroundTaskRegistry 测试覆盖重复启动、阶段进度、失败、取消和终态投影；前端 provider
  测试覆盖事件更新、漏事件轮询和取消。
- fresh schema contract test 断言两套 Question 表仅含：`tenant_id`、`id`、`session_id`、
  `title`、`created_at`、`updated_at`。
- 代表性 legacy Question fixture migration test 验证六个旧字段被移除、标题回填、审计快照
  保留；fresh contract test、audit/repair/rollback 临时数据库测试均通过。
- Content Node projection test 验证单节点使用源 Part ID，多节点使用稳定 `-node-N` 后缀，
  单节点旧锚点进入 `legacy_anchor_ids`。

## 性能基线

基线环境：本地 macOS、Rust test profile、Node 22/pnpm 10；数据为脱敏临时 SQLite fixture。

| 工作负载 | 基线命令 | 实测结果 |
|---|---|---:|
| Rust 全量单元/集成测试（695 tests，1 ignored） | `cargo fmt --all -- --check && cargo test --workspace` | 126.09s（测试阶段） |
| 前端 typecheck 与全量测试（110 files，558 tests） | `pnpm typecheck && pnpm test` | 63.24s |
| 前端生产构建与 artifact guard | `pnpm build` | 14.44s |
| Go vet、race tests、CLI contract 与 CLI–Engine e2e | `go vet -C cli ./...`、`go test -C cli -race ./...`、`ASSETIWEAVE_DB_PATH=... pnpm cli:contract`、`ASSETIWEAVE_DB_PATH=... pnpm cli:test:e2e` | PASS |

上述结果是施工机基线，不作为跨机器硬阈值。当前仓库没有独立的大会话性能 fixture，
因此没有虚构读取延迟、搜索吞吐或历史修复吞吐数字；后续性能优化应使用同一 fixture 和
命令补充比较。

## 回滚说明

apply 前的数据库备份由现有 backup 设置决定；返回值包含 `backup_path` 和
`conversation.data.rollback` 入口。rollback 会 checkpoint WAL、替换活动数据库并返回
`requires_app_restart: true`，重启后由迁移器重新打开数据库。
