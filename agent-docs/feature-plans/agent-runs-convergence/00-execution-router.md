# 执行路由：Issue #75 与 #76 的边界澄清，及 #75 在新目录下的续推

## 0. 先分清两件事（本文件的存在理由）

仓库里正在并行推进两个**互相独立但物理重叠**的 Issue，极易混淆：

| | Issue #75 | Issue #76 |
|---|---|---|
| 标题 | 统一 Agent 运行工作区至 `~/.assetiweave/agent-runs` 并阻断临时会话入库 | 业务纵切重构 Rust 后端双 Crate（`agent_engine` / `asset_core`）与 `desktop` 桌面宿主壳规整 |
| 性质 | **功能收口**（防污染、工作区路径） | **物理目录重构**（`src-tauri` → `desktop` + `backend/*`） |
| 状态 | CLOSED（已声明完成） | **OPEN（仍在推进中）** |
| 创建时间 | 2026-10-07 10:31 | 2026-10-07 14:46（**晚于 #75**） |
| 是否改路径 | 否 | **是 —— 它把 #75 落地代码所在的文件全部搬了家** |

**这就是歧义的根源**：#75 正文与完成报告里写的全是 `src-tauri/**` 路径；随后 #76 把这些文件搬到了 `backend/**` 与 `desktop/**`。
于是 #75 的每一条路径引用都失效了，但 #75 的 Issue 正文并没有、也不应该为此更新。

**分工结论**：

- 想继续推 **#75 的功能收口** → 读本文第 2、3 节（T01–T23）。
- 想继续推 **#76 的目录重构收尾** → 读本文第 4 节。
- 两者共用第 1 节的路径映射表。

---

## 1. 路径映射表（#76 造成，两个 Issue 共用）

| #75 / 历史文档中的旧路径 | 新路径（当前权威） |
|---|---|
| `src-tauri/src/backend/store/**` | `backend/asset_core/src/store/**` |
| `src-tauri/src/backend/application/**` | `backend/asset_core/src/application/**` |
| `src-tauri/src/backend/infrastructure/**` | `backend/asset_core/src/infrastructure/**` |
| `src-tauri/src/backend/**`（ACP / 进程 / 协议 / 沙箱） | `backend/agent_engine/src/**` |
| `src-tauri/src/adapters/**` | `desktop/src/adapters/**` |
| `src-tauri/src/lib.rs` / `main.rs` / `bin` | `desktop/src/lib.rs` / `main.rs` / `bin` |
| `src-tauri/migrations/**` | `backend/asset_core/migrations/**` |
| `src-tauri/tauri.conf.json` / `tauri.windows.conf.json` | `desktop/tauri.conf.json` / `desktop/tauri.windows.conf.json` |
| `src-tauri/test-fixtures/**` | `desktop/test-fixtures/**` 与 `backend/asset_core/test-fixtures/**` |
| `crates/agent_engine/**` | `backend/agent_engine/**` |

**未受影响、路径保持有效**：`builtin-assets/adapters/**`、`cli/**`、`frontend/**`、`scripts/**`。

根 `Cargo.toml` workspace 成员已收口为 `desktop`、`backend/agent_engine`、`backend/asset_core`。

---

## 2. #75 已完成基线（新路径下的验收锚点，不得回退）

| 能力 | 权威位置（新路径） |
|---|---|
| 工作区根目录计算 | `backend/asset_core/src/infrastructure/agent_execution/mod.rs:29-45` |
| `ASSETIWEAVE_AGENT_RUNS_DIR` 覆盖 | 同上 `:30` |
| `ASSETIWEAVE_HOME` 分支 | 同上 `:36-41` |
| 业务分类枚举 `AiExecutionPurpose` | `backend/asset_core/src/infrastructure/agent_execution/types.rs:123-135` |
| 分类目录映射 `category_folder()` | 同上 `:137-149` |
| 分类落盘 `create_workspace_for_purpose` | `.../agent_execution/backends/acp_prompt.rs:190-197` |
| 唯一装配点 | `backend/asset_core/src/infrastructure/runtime/app_runtime.rs:82-86` |
| 后端分发 | `backend/asset_core/src/infrastructure/agent_market/runtime/mod.rs:75-76` |
| 存储层沙箱判定 | `backend/asset_core/src/store/conversations/conversation_repo/common.rs:73-80` |
| 导入丢弃判定 | 同上 `:82-88`（`is_droppable_internal_session`） |
| 批量导入拦截 | `.../conversation_repo/import.rs:108-119` |
| 并发导入拦截 | `.../conversation_repo/import_advanced.rs:120-125`、`:167-175` |
| 单会话深度防御 | 同上 `:422-424` |
| 适配器过滤 | `builtin-assets/adapters/{codex,opencode,claude-code,antigravity}/adapter.mjs` |

旧路径 `~/Library/Application Support/AssetIWeave/agent-executions/` 的计算逻辑**已在生产代码中彻底移除**。
现存 `agent-executions` 命中均为**历史数据兼容过滤**（`common.rs:73-80` 与四个适配器），属有意保留，**不得当作遗漏清理**。

---

## 3. #75 未彻底之处（续推清单 T01–T23，全部新路径）

### P0 — 可被污染入库的真实漏洞

| ID | 问题 | 位置 | 影响 |
|---|---|---|---|
| T01 | **codebuddy 适配器零过滤**，`assetiweave` 关键字在文件内出现 0 次；且是默认启用的内置 live source（`common.rs:55-67`，`id: "codebuddy-live"`） | `builtin-assets/adapters/codebuddy/adapter.mjs:766-800, 802+` | 内部沙箱会话完整落库 |
| T02 | **zcode 适配器零过滤**，`sessionRows` 无 WHERE 过滤、read_session 直接 push | `builtin-assets/adapters/zcode/adapter.mjs:564-586, 1049-1067` | 同上；且该适配器无测试文件 |
| T03 | `project_path` 为 `None` 时判定短路返回 `false`，双层防线同时失效（放大 T01/T02） | `backend/asset_core/src/store/conversations/conversation_repo/common.rs:74-76` | 兜底层被击穿 |
| T04 | `reproject` 是全库唯一无判定的 `conversation_sessions` 写入路径 | `.../conversation_repo/reproject.rs:16-18` | 重放投影把存量行改写成沙箱路径 |
| T05 | `is_droppable_internal_session` 只看 `project_path`，不检查 `source_locator` 与 `execution_origin` | `common.rs:82-88` | 路径缺失即可穿透 |

### P1 — 分类目录与行为不一致

| ID | 问题 | 位置 |
|---|---|---|
| T06 | **`teams` 类别完全未落地**：枚举无对应变体，`category_folder()` 无 `teams` 分支，`backend/` + `desktop/src` 零命中 | `types.rs:123-149` |
| T07 | Native 后端绕过分类，直接建在 root 下 | `backends/native.rs:55` |
| T08 | ACP 探活/模型发现（本应落 `general/`）绕过分类 | `backends/acp_probe.rs:199, 246, 296` |
| T09 | 两套 `create_workspace` 行为不一致：ACP 用 `execution-` 前缀 + 绝对路径校验 + `Workspace` 错误；Native 用 `exec-` 前缀、无校验、`Output` 错误 | `acp_prompt.rs:173-188` vs `native_runner.rs:368-374` |
| T10 | 第三层自定义子目录 `conformance/` | `backend/asset_core/src/infrastructure/agent_market/materialize.rs:367, 370, 407, 410` |

### P2 — 过滤语义三套并存（最易误改）

当前仓库存在**互不相同**的三套内部会话判定：

1. **导入层**：子串黑名单 `{agent-executions, agent-runs, .assetiweave}` — `common.rs:73-80`
2. **Recent Work / Session Memory 层**：仅 `agent-runs` **单一根前缀**，不认 `agent-executions`，也不认任意位置的 `.assetiweave` — `.../conversation_repo/sessions.rs:122-129`、memory `enqueue.rs:109-115` 与 `:203`、`recent_snapshot_pipeline/candidates.rs:19-25`
3. **根路径重算**：4 处各自重复调用 `resolve_agent_execution_workspace_root()` — `recent.rs:71`、`event_handlers.rs:79,102`、`scheduler.rs:26`、`candidates.rs:54,85`

| ID | 问题 | 说明 |
|---|---|---|
| T11 | 历史 `agent-executions` 沙箱与任意位置的 `.assetiweave` 不被 Recent/Memory 排除 | 泄漏进 Recent Work 快照与 Session Memory 作业队列 |
| T12 | 环境变量被改写时 root 与真实沙箱路径不一致，前缀匹配整体失效 | 依赖 `ASSETIWEAVE_AGENT_RUNS_DIR` / `ASSETIWEAVE_HOME` 一致性 |
| T13 | 4 份 `isInternalExecutionPath` 逐字复制未共享：codex/opencode/claude-code 是 6 条含裸子串；antigravity 是 7 条**仅斜杠包裹**，语义更弱，仅靠 `/.assetiweave/` 侥幸命中 | `builtin-assets/adapters/*/adapter.mjs` |
| T14 | `claude-code` 的 `listSessions` 完全不判（函数就在紧邻的 826 行） | `claude-code/adapter.mjs:839-851` |
| T15 | `antigravity` 的 `listSessions` 两条主路径不判 | `antigravity/adapter.mjs:1776-1801, 1806-1828` |
| T16 | codex/opencode 的 SQL 过滤是**条件性**的（仅当 cwd/directory 列存在时才加 WHERE） | `codex/adapter.mjs:1807-1809`、`opencode/adapter.mjs:631-633` |
| T17 | `dry_run` 分支不判也不计 skipped | `import.rs:62-81`、`import_advanced.rs:25-53` |

### P3 — 测试缺口

| ID | 问题 | 位置 |
|---|---|---|
| T18 | 唯一 Rust 测试只覆盖 `import_conversation_sessions_sqlx`，**生产实际走 `import_conversation_sessions_advanced_sqlx`**（`backend/asset_core/src/application/conversations/conversation_sync_support.rs:190`），该路径零覆盖 | `conversation_repo_tests.rs:3175-3250` |
| T19 | 适配器侧 sandbox 过滤测试为 0 条；zcode 连测试文件都没有 | `scripts/*-conversation-adapter.test.mjs` |
| T20 | 未测 `ASSETIWEAVE_HOME` 分支与默认分支 | `.../agent_execution/agent_execution_tests.rs:8,18` |

### P4 — 配置层绕道（非污染，但属收口范围）

| ID | 问题 | 位置 |
|---|---|---|
| T21 | `ASSETIWEAVE_AGENT_RUNS_DIR` 不在 `RuntimeConfig` 环境变量白名单内，agent 工作区 root 绕开统一配置层 | `backend/asset_core/src/infrastructure/runtime/config.rs:149-154` |
| T22 | `ASSETIWEAVE_HOME` 在 `mod.rs:36-41` 与 `config.rs:59,70-74,87-90` 各实现一遍；非 UTF-8 处理策略不同（`env::var` vs `var_os`） | 见上 |
| T23 | override 值未规范化：无 `~` 展开、无绝对路径强制 | `mod.rs:33` |

### ⚠️ 已知误伤风险（改动 T05 时必须规避）

`backend/asset_core/src/application/memory/memory_recall_executor.rs:446-471` 使用
`project_path = session.scope.project_path` + `execution_origin = "internal_memory"` 调用导入。
若 Recall 的 scope 落在 `.assetiweave` / `agent-runs` 下，会被 `is_droppable_internal_session` 判掉 →
**Recall 结果静默丢失**（计为 `skipped` 而非报错）。
扩大判定范围时必须为 Recall 通道开显式豁免，**不得把 `internal_memory` 一律当作可丢弃**。

---

## 4. Issue #76 的收尾状态（目录重构）

### 4.1 已完成（已实测确认）

| #76 验收项 | 状态 | 证据 |
|---|---|---|
| `src-tauri` → `desktop/` | ✅ | `src-tauri/` 已不存在；`desktop/` 含 `src/`、`tauri.conf.json`、`test-fixtures/` |
| `crates/agent_engine` → `backend/agent_engine` | ✅ | 根 `crates/` 已移除 |
| `src-tauri/src/backend` → `backend/asset_core` | ✅ | 含 `application`/`domain`/`infrastructure`/`store` 四层 |
| 根 `Cargo.toml` workspace 成员 | ✅ | `desktop`、`backend/agent_engine`、`backend/asset_core` |
| `desktop/tauri.conf.json` 单级偏移（US7） | ✅ | `frontendDist: "../dist"`，`beforeDevCommand: "pnpm dev"` |
| 脚本契约对齐 | ✅ | 见下方 4.2 |
| `cargo fmt --all -- --check` | ✅ 通过 | 5s，退出码 0 |

### 4.2 脚本门禁的适配方式（注意：不是遗漏，是有意兼容）

`scripts/` 中残留的 `src-tauri` 字面量**均已做「新路径首选 + 旧路径回退」**，无需清理：

- `scripts/check-backend-line-limits.mjs:22-30` — 首选 `backend/asset_core/src` + `backend/agent_engine/src`，二者均不存在时才回退旧路径
- `scripts/check-backend-layer-dependencies.mjs:114-118` — 同模式
- `scripts/check-backend-root-layout.mjs:10-13` — 首选 `backend/asset_core/src/lib.rs`，回退 `src-tauri/src/backend/mod.rs`
- `scripts/check-module-boundaries.sh:68-80` — 显式 if/else，注释说明「防止移动/删除的旧路径静默禁用边界规则」
- `scripts/conversation-adapter-packages.mjs:159-166` — `src-tauri/builtin-assets` 与 `src-tauri/bundled/conversation-adapters` 是**断言「不应存在」的黑名单**，用于防止旧路径复活，**必须保留**

### 4.3 #76 真正剩余项

| ID | 问题 | 位置 |
|---|---|---|
| R76-1 | 前端测试仍断言旧结构：「当前仓库包含 `frontend`、`src-tauri` 和 `cli` 三个核心模块」 | `frontend/src/components/agent-session/AgentSessionWorkspace.acceptance.test.tsx:57` |
| R76-2 | 前端测试硬编码旧路径 `./src-tauri/src/path_utils.rs:166`（两处断言） | `frontend/src/pages/conversations/ConversationsPage.test.tsx:1115, 1137` |
| R76-3 | vite watch ignored 仍列 `**/src-tauri/**`、`**/crates/**`（无害，可清理） | `frontend/vite.config.ts:26` |
| R76-4 | `cargo test --workspace` 未验证（#76 Seam 4 要求） | 待跑 |
| R76-5 | **`#76` 的 Further Notes 只声明「保护 Task Center 相关未提交改动」，未提及 #75** | 交叉风险，见第 5 节 |

---

## 5. ⚠️ 交叉风险：两个 Issue 都未提交，且物理重叠

| 风险 | 事实 | 后果 |
|---|---|---|
| C1 | #75 落地代码从未 commit：`git grep "agent-runs" HEAD` 与 `git grep "is_internal_execution_workspace_path" HEAD` **均为空** | #75 只存在于工作区 |
| C2 | #76 重构也未 commit：`git status` → `730 D / 11 ?? / 66 M`；`backend/`、`desktop/` 仍是未跟踪 | 磁盘与 HEAD 完全脱节 |
| C3 | #76 的物理移动范围覆盖了 #75 的全部落地文件 | #75 的成功能否存活，完全取决于 #76 的搬迁是否完整 —— 目前看是完整的（第 2 节所有锚点均在新路径下存在），但**没有任何 git 快照可以证明或回滚** |
| C4 | 两者混在同一个未提交工作区中，无法通过 git 区分 | 无法单独回滚其中一个 |

**结论：在任何功能推进之前，必须先做基线提交。**

```bash
cd /Users/util6/code-space/assetiweave
git status --short            # 复核 730 D / 11 ?? / 66 M
git add -A backend desktop    # 先把未跟踪新目录纳入索引
```

建议拆成两个原子提交：

- `refactor(repo): 将 src-tauri 拆分为 backend/agent_engine、backend/asset_core 与 desktop 宿主壳 (#76)`
- `feat(agent-runs): 统一 Agent 执行工作区至 ~/.assetiweave/agent-runs 并建立双层防污染 (#75)`

提交后验证：`git grep -c "agent-runs" HEAD -- backend/` 应大于 0。

---

## 6. 续推顺序建议

1. **S00（阻塞项）**：基线提交，解除 C1–C4。
2. **S01**：#76 收尾 R76-1 / R76-2（前端旧路径断言），跑 `cargo test --workspace` 验证 R76-4。
3. **S02**：#75 P0 止血 T01–T05。
4. **S03**：#75 统一语义 T13 + T11/T12（三套判定收口为一处）。
5. **S04**：#75 分类补齐 T06–T10。
6. **S05**：#75 测试补齐 T18–T20（按 `AGENTS.md` §4 落到 `*_tests.rs`）。
7. **S06**：#75 配置层 T21–T23。

## 7. 文档导航与事实源优先级

- 本文：#75 / #76 边界澄清 + 续推清单（**推进入口**）
- `agent-docs/feature-plans/acp-agent-execution-runtime/`：ACP 执行运行时原始设计
- `agent-docs/feature-plans/backend-architecture-convergence/`：后端架构收口

优先级：代码与测试事实 > **本文** > GitHub Issue #75 / #76 原文 > `CONTEXT.md` > 其他 ADR。
凡本文与 #75 原文冲突（尤其是路径），一律以本文为准。
