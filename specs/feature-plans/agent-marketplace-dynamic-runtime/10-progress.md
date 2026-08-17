# Progress：Agent Market 与动态 Runtime

| 字段 | 值 |
|---|---|
| 更新时间 | 2026-08-16 |
| SPEC 状态 | Proposed，文档编写完成，待人工评审 |
| 实施状态 | 未开始 |
| 当前任务 | T00 人工评审并冻结 SPEC |
| 代码基线 | `01ccbdf` |

## 1. 本轮完成内容

已生成完整 SPEC 文档集：

- [x] 主索引、现状审计和冻结决策。
- [x] 产品需求、用户旅程、功能/非功能需求和验收场景。
- [x] 分层架构、动态 Registry、Runtime ownership、并发和 ADR。
- [x] Curated catalog 及 System/Binary/Npx/Uvx 契约。
- [x] 安装/更新/重装/卸载/恢复状态机和错误模型。
- [x] OpenCode System/Binary 双兼容及 CLI 兜底语义修正。
- [x] SQLite、AppService、Tauri、Engine、CLI、Frontend 和迁移契约。
- [x] 安全边界、资源预算、测试矩阵、质量门和发布验收。
- [x] T00–T33（含 T15A）增量实施任务。
- [x] Lunna/Flash 单任务执行和评审模板。

本轮未修改产品代码、数据库 migration、前端或 generated contract。

## 2. 代码审计事实

| ID | 事实 | 证据位置 |
|---|---|---|
| A-01 | 后端固定注册九个 Agent | `src-tauri/src/backend/agents/registry.rs` |
| A-02 | 前端重复维护静态目录 | `frontend/src/components/settings/agentCatalog.ts` |
| A-03 | Registry 构造后 definitions 不可动态替换 | `src-tauri/src/backend/agents/registry.rs` |
| A-04 | Agent Runtime 使用进程级 `OnceLock` | `src-tauri/src/backend/ai_execution/mod.rs` |
| A-05 | active execution 只记录 cancellation，不记录 Agent | `src-tauri/src/backend/ai_execution/executor.rs` |
| A-06 | OpenCode `cli_fallback` 只改变 connection result | `src-tauri/src/backend/ai_execution/executor.rs` |
| A-07 | Translation 生产路径仍为 ACP，不走 `opencode run` | `src-tauri/src/backend/application/card_translation.rs` + Agent executor/backend |
| A-08 | Claude/Codex/Pi 当前 runtime definition 含 `npx -y` | `src-tauri/src/backend/agents/registry.rs` |
| A-09 | Agent AppService 目前只有 catalog/connection/models | `src-tauri/src/backend/application/agent.rs` |
| A-10 | Tauri 已有通用 BackgroundTaskRegistry 模式 | `src-tauri/src/adapters/tauri/background_tasks.rs` |
| A-11 | Engine 已注册现有 Agent catalog/connection/models | `src-tauri/src/adapters/engine/registry.rs` |
| A-12 | Conversation package 域已有复杂 hash/trust/history，Agent Market 不应复制 | `src-tauri/src/backend/application/conversation_script_catalog.rs` 与 conversation migrations |

## 3. 外部生态结论

| ID | 结论 |
|---|---|
| E-01 | ACP 官方 Registry format 支持 Binary、Npx、Uvx。 |
| E-02 | OpenCode 官方 Registry 是平台 Binary，并以 `acp` 参数启动。 |
| E-03 | Claude/Codex/Pi 等可通过 Npx 分发，但 Npx 分发不代表实现必然是纯 Node。 |
| E-04 | Hermes 可通过 Python/PyPI/uv tool 分发。 |
| E-05 | 当前 Kiro 文档使用 `kiro-cli acp`；Qoder 文档使用 `qoder --acp`，官方 Registry 另提供固定 Npx 包。它们均与仓库旧 hardcoded command/单分发假设存在漂移。 |
| E-06 | Antigravity 为现有 Native Agent，是 ACP-only 产品抽象的例外。 |

外部版本会变化；实施 T04 时必须基于当时官方数据生成固定 bundled catalog 和 smoke evidence，不直接复制本进度文档中的历史版本。

## 4. 决策记录

| 决策 | 状态 | 备注 |
|---|---|---|
| Agent Market（ACP + Native） | Proposed | 待 T00 评审 |
| Curated pinned index | Proposed | 官方 latest 仅作上游 |
| System/Binary/Npx/Uvx | Proposed | 一个 Agent 多 distribution |
| Single active installation | Proposed | 无历史/rollback UI |
| Manual update | Proposed | 无静默升级 |
| 轻量 integrity | Proposed | 无 recursive hash/trust state |
| Runtime network-free | Proposed | 无 runtime npx -y/uvx |
| OpenCode ACP-only execution | Proposed | System/Binary 只影响分发 |
| No custom packages/editing | Proposed | MVP 非目标 |

## 5. Task 状态

| Phase | Tasks | 状态 |
|---|---|---|
| 0 决策/基线 | T00–T01 | Pending |
| 1 领域/Store | T02–T03 | Pending |
| 2 Catalog/选择 | T04–T06 | Pending |
| 3 Installers | T07–T10 | Pending |
| 4 Runtime | T11–T15A | Pending |
| 5 Lifecycle | T16–T19 | Pending |
| 6 API/CLI | T20–T24 | Pending |
| 7 迁移/OpenCode | T25–T26 | Pending |
| 8 Frontend | T27–T31 | Pending |
| 9 清理/验收 | T32–T33 | Pending |

## 6. 当前工作区保护

编写本 SPEC 时检测到以下既有未提交修改，它们不属于本规格文档任务，未被改写：

```text
builtin-assets/adapters/codex/adapter.mjs
builtin-assets/adapters/codex/conversation-adapter-package.json
builtin-assets/adapters/codex/conversation-adapter.json
builtin-assets/adapters/codex/payload-policy.mjs
builtin-assets/catalog.json
builtin-assets/history/io.github.util6.codex-session.json
builtin-assets/index.json
scripts/codex-conversation-adapter.test.mjs
specs/feature-plans/SPEC_ ACP Agent Execution Runtime.md
src-tauri/src/backend/conversations/tests.rs
specs/feature-plans/SPEC_ 前端统一 Skeleton 架构.md
specs/feature-plans/skeleton-rendering-flicker/
```

后续执行模型必须在每个 Task 开始时重新运行 `git status --short`，不能假设列表保持不变。

## 7. 文档自检证据

```text
- structure/fence/relative links: PASS（11 files，10 relative links）
- requirement/task/decision IDs: PASS（56 requirements，35 tasks，16 decisions）
- user-home absolute path check: PASS（无用户目录绝对路径；展示路径使用 ~）
- whitespace/git diff --check equivalent: PASS
- current official source recheck: PASS（ACP Registry format、OpenCode/Gemini/Claude/Codex/Pi/Qoder、Kiro/Qoder docs；2026-08-16）
- product code changed by this SPEC task: NO（仅新增本 SPEC 索引与目录）
```

## 8. T00 人工评审清单

- [ ] 同意产品命名和 ACP/Native 边界。
- [ ] 同意官方 Registry -> AssetIWeave curated index -> client 的数据链。
- [ ] 同意首版 System/Binary/Npx/Uvx。
- [ ] 同意不下载/维护 Node、Python、uv Runtime。
- [ ] 同意单 active version、手动更新、无 rollback history。
- [ ] 同意轻量完整性，不维护 recursive hash/trust state。
- [ ] 同意 System bind 可保留 ACP failed 诊断 row，但不进入 Registry。
- [ ] 同意 OpenCode 无 `opencode run` execution fallback。
- [ ] 同意只迁移已 assignment Agent，不静默联网安装。
- [ ] 同意资源上限和 10 分钟 lifecycle timeout。
- [ ] 同意 T00–T33（含 T15A）的阶段与文件边界。

## 9. 下一步

下一步唯一任务：**T00 人工评审并冻结 SPEC**。

T00 完成前不应启动产品代码实施。评审有修改时先同步主索引、01/02 及受影响契约，再更新实施任务，避免代码模型面对冲突规范。

## 10. Task 证据追加区

后续按以下格式追加，不覆盖历史：

```text
### YYYY-MM-DD · TXX TASK_TITLE

- Status:
- Commit/working tree:
- Files:
- Tests-first evidence:
- Verification:
- Acceptance:
- Deviations:
- Next:
```

### 2026-08-17 · T02–T33 implementation checkpoint

- Status: 已完成主要实现，进入最终质量门；SPEC 状态仍为 Proposed，T00 人工评审保持 Pending。
- Commit/working tree: 当前分支 `codex/agent-marketplace-dynamic-runtime` 的工作区实现；未改写 SPEC 目录外已存在的无关修改。
- Files: 新增 Agent Market domain/catalog/cache/installers/lifecycle/migration、SQLite migration、bundled catalog、动态 Registry/runtime manager、AppService/Tauri/Engine/CLI/Frontend 集成及生命周期任务/预览对话框。
- Tests-first evidence: 覆盖 System/Binary/Npx/Uvx 选择与安装约束、缓存/Registry 原子替换、生命周期任务去重/取消/terminal retention、CLI preview/confirmation、前端 lifecycle provider 与卸载清理确认。
- Verification: `pnpm typecheck`、`pnpm test`（99 files/514 tests）、`pnpm build`、Rust targeted Engine/DB/background tests、Rust full suite（576 passed/1 ignored）、CLI targeted tests 和 `go vet` 已通过；`git diff --check`、相关 Rust `rustfmt --check` 已通过。CLI full test/race 仍受当前沙箱禁止 IPv6 `httptest` listener 和 fake Engine 进程启动影响。
- Acceptance: bundled catalog 可在未安装九个 Agent 时展示；安装记录按 tenant 持久化；动态 Registry 只加载 enabled + ready + execution-ready；System/Binary/Npx/Uvx 走统一生命周期；managed uninstall 删除 app-owned 文件，System uninstall 只解除绑定；运行中 Agent 的更新/卸载受 mutation gate 阻止；Tauri/Engine/CLI contract 已同步；前端 Market/Installed、安装/更新/重装/卸载预览和后台任务状态已接入。
- Deviations: 安装 protocol conformance 尚未把生命周期取消令牌深入 ACP/Native probe；legacy assignment migration 当前在 AppService open 中同步执行；catalog remote fetch 尚未提供完整可注入 fetcher/clock/filesystem seam 与 304 远端 fixture；Tauri lifecycle task 查询/取消接口保留为 desktop-only，CLI 使用已确认的一次性 Engine lifecycle 方法；T00 人工评审、真实平台 Binary/Npx/Uvx 联机 smoke 和最终人工 UI review 仍待完成。
- Next: 完成代码 review/平台 smoke；确认工作区保护清单后提交中文 commit。
