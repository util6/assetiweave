# Memory 重写：T01–T15 交付记录

- 日期：2026-09-01
- 入口：`00-execution-router.md`
- 父 Issue：[#20](https://github.com/util6/assetiweave/issues/20)
- 实施状态：**Implemented**
- 汇总提交：`bc5c14e feat: 完成 Memory 重写与跨层验收`

## 1. Ticket 结果

| Ticket | 结果 | 代码/证据 |
|---|---|---|
| T01 | PASS | `f513c20`；Recent 72h、last activity、项目目录解析、tenant 与跨 worktree 测试。 |
| T02 | PASS | `55a1d82`；Conversation commit → durable Job → Session Memory、redaction、revision 幂等和 Recent Event 测试。 |
| T03 | PASS | `31b0779`；lease、heartbeat、retry、restart、cancel、TaskRuntime 投影和启动恢复测试。 |
| T04 | PASS | `e028440`；项目目录合并、输入 fingerprint、同项目串行、失败保留 last-success、文档原子发布测试。 |
| T05 | PASS | `07c247b`；Global last-success、预算优先级、context revision、app-owned 文档和失败保留测试。 |
| T06 | PASS | `43d4514`；修订/删除/缺失/项目迁移/排除/contract 变化的失效传播与投影重建测试。 |
| T07 | PASS | `f0a7bf0`；近期项目/时间视图、自然语言投影、Markdown、Session/content 精确导航和 DOM 负断言测试。 |
| T08 | PASS | `1abe8e7`；filter、lexical、deterministic semantic、rerank、去重、scope 与只读工具测试。 |
| T09 | PASS | `bc5c14e`；Recall Session/Turn、四字段结构化输出、引用验证、Conversation 持久化和 one-turn 测试。 |
| T10 | PASS | `bc5c14e`；多轮顺序、persistent binding、取消、late output、恢复/回放和结构化错误测试。 |
| T11 | PASS | `bc5c14e`；usage 幂等、Context/Recall 真实采用点、生成/使用开关、排除和四类 assignment 测试。 |
| T12 | PASS | `bc5c14e`；Tauri/Engine/CLI/Skill 新合同、生成 contract、CLI maintenance、surface parity 和旧方法退出检查。 |
| T13 | PASS | `bc5c14e`；Memory 仅保留近期/回忆、settings service、全局 task provider、event + polling 和响应性测试。 |
| T14 | PASS | `bc5c14e`；app-owned 只读归档、历史 migration 不变、旧 UI/API/CLI/Skill/后台路径退出与 legacy 分类。 |
| T15 | PASS（自动化） | `bc5c14e`；全仓 Gate、迁移、tenant、redaction、取消、重启、契约和构建证据汇总如下。 |

## 2. 主要实现切片

### Durable Memory

- 新增 `memory_recall_sessions`、`memory_recall_turns`、`memory_usage_events` migration；
  已发布 migration 未修改。
- Session/Project/Global repository 保留 SQLite authority；TaskRuntime 只保存活动执行投影。
- AppRuntime 启动执行漏单、过期 lease、retry 和 Memory task 恢复，不依赖打开 Memory 页面。
- Project/Global Markdown 只写入 app-owned workspace，使用临时文件和原子 rename；失败不覆盖
  last-success。

### Recall 与检索

- Memory search 对 Conversation source 做 scope filter、lexical + deterministic semantic
  candidate merge 和稳定排序，结果再由 SQLite hydrate。
- Recall 采用持久 Session/Turn；Agent 只能通过只读 Memory MCP 工具访问候选和内容。
- 结构化输出引用必须验证 tenant、record kind、session、question、turn/part/block 关系；
  非法或跨 scope 引用使结果失败，不静默展示。
- Recall 消息和回答复用 Conversation 合同，Memory 表只保留 workflow/reference metadata。

### Public surface 与 Desktop

- Engine/CLI/Tauri/frontend service 已同步 `memory.recent.*`、`memory.context.*`、
  `memory.project.*`、`memory.rebuild`、`memory.task.*` 和 `memory.recall.*`。
- 旧 Dream/Library/候选/Evidence 产品表面已从 router、组件、registry、CLI、Skill 和后台
  触发删除；旧表只进入一次性只读归档。
- Settings 持久化 Memory generation/use flags、排除规则和 extraction/project/global/recall
  四类 assignment。
- MemoryTaskProvider 在 AppProviders 根部运行，使用 task event 加 1 秒 polling；取消/重试只
  影响冲突任务，导航、筛选和无关 CRUD 保持可用。

## 3. 验证记录

```text
cargo fmt --all -- --check                                       PASS
RUSTFLAGS='-Awarnings' cargo test --workspace --no-default-features -- --test-threads=1
                                                                 PASS: 742 tests
pnpm typecheck                                                   PASS
pnpm test                                                        PASS: 114 files / 569 tests
pnpm build                                                       PASS
go vet -C cli ./...                                              PASS
go test -C cli -race ./...                                       PASS
pnpm check:boundaries                                            PASS
pnpm test:boundaries                                             PASS
pnpm gen:surface-matrix && pnpm check:surface-matrix              PASS: 42 exemptions
python3 scripts/memory-skill-recall.test.py                      PASS: 4 tests
node scripts/check-agent-catalog-release.mjs --static             PASS
node scripts/check-agent-catalog-release.mjs --release --network  PASS
node scripts/check-agent-catalog-release.mjs --release --e2e      PASS
```

Contract 生成使用临时数据库，避免读取/修改用户数据库中的历史 migration checksum：

```bash
ASSETIWEAVE_DB_PATH=/tmp/assetiweave-contract.sqlite pnpm cli:contract
```

该命令连续执行后 `cli/internal/schema/contract.json` 字节一致。

## 4. Legacy grep 分类

| 命中位置 | 处置 |
|---|---|
| `src-tauri/migrations/` 的旧表、旧状态和 migration SQL 变量 | 保留。已发布历史 migration 不改写。 |
| `src-tauri/src/backend/runtime/memory_legacy_archive.rs` | 保留。只读、幂等、app-owned 归档 reader。 |
| `src-tauri/src/backend/app_settings.rs` 的 `memory.dream` | 保留。仅用于旧设置迁移和负向测试，不进入新 assignment。 |
| `agent-docs/feature-plans/memory-rewrite/` | 保留。属于执行规格和历史词汇，不是用户/Engine public surface。 |
| Skill 路径发现循环中的 `candidate` | 保留。是 Python 变量名，不是旧 Memory 合同。 |
| Agent Market、Skill discovery、Conversation projection 中的 `candidate`/`evidence` | 保留。属于其他领域的内部实现语义，不是 Memory 旧 public surface。 |

当前 active registry、Tauri handler、Go Memory command、frontend router 和 builtin Memory Skill
均不再暴露 `memory.dream.*`、`memory.recall.preview`、`memory.recall.run` 或旧 Library/Item
管理接口。

## 5. 发布补充

自动化 Gate 已通过。真实桌面视觉、窗口关闭提示和多步骤操作仍按 T15 的六步作为发布前
人工 smoke 项，不把人工观察伪写成自动化测试结果；代码层面的 task 状态、取消、路由、scope
和响应性契约已有测试覆盖。
