# Feature Plans 实施状态

更新时间：2026-09-07

本文是 `agent-docs/feature-plans/` 的实施状态索引。各计划原文保留需求、约束和历史
执行记录；当前代码、测试、Engine contract 和 Git 历史是实现事实的最终来源。

## 1. 总体结论

当前工作区中的 feature-plans 已完成对应产品代码落地，跨层入口已经收口到
AppService/backend；Frontend 通过 `frontend/src/services/`，Go CLI 通过 Rust Engine，
生成 contract 由命令产生。Memory 重写的入口为
`agent-docs/feature-plans/memory-rewrite/00-execution-router.md`，T01–T15 已在
`bc5c14e` 及其前序提交中落地。

## 2. 计划族状态

| 计划族 | 状态 | 实现事实与证据 |
|---|---|---|
| `memory-rewrite` | **Implemented** | T01–T08：`f513c20`、`55a1d82`、`31b0779`、`e028440`、`07c247b`、`43d4514`、`f0a7bf0`、`1abe8e7`；T09–T15 与旧表面切换：`bc5c14e`。 |
| `team-multi-agent` | **Implemented** | Team roster、review/confirm、execution、mailbox、导航和后台任务：`5e666bf`、`6157d66`、`141b493`。 |
| `acp-agent-execution-runtime` | **Implemented** | ACP/Native runtime、persistent binding、取消、清理、错误和 OneShot/持久执行接入；现行 Rust 全量测试覆盖。 |
| `agent-marketplace-dynamic-runtime` | **Implemented** | Catalog、安装生命周期、动态 runtime、跨租户任务、Frontend/CLI/Engine 接入，以及 release/static/network/real ACP E2E 校验。 |
| `backend-architecture-convergence` | **Post-audit remediation** | 核心 Authority 已接入；当前 Agent Catalog release evidence 哈希与 bundled Catalog 不一致，Issue #1 保持 OPEN。 |
| `backend-infrastructure-convergence-v2` | **Implemented** | 审计后纠偏 B2-R16–G03 全量完成；GitHub Actions CI [Run 34050901408](https://github.com/util6/assetiweave/actions/runs/34050901408) 5/5 jobs 成功；Linux 897 passed，Windows 807 passed（包含 Job Object 后代管道回收测试），Go/Frontend/E2E 全绿，Issue #24 达成最终重新验收并关闭。 |
| `backend-error-flow-convergence` | **Implemented** | 错误流收口 ERR-00–G01 全量完成；消除跨模块 `Result<T, String>`（生产跨模块接口 0 残留，精确保留 6 处最外层 transport、5 处私有校验/解析器与 2 处 test-only helper）；引入类型化 `MenuRepoError`、`AgentMarketError`、`LogSnapshotError`、`ProjectionError` 并保留 typed source；收紧 `AppError` 与 `WireError` 双适配器 parity；通过脱敏审计与全量测试门禁，Issue #2 达成终态验收并关闭。 |
| `runtime-extension-refactor` | **Implemented** | Runtime、事件 outbox、extension kernel、能力边界和 interface coverage 已由生产 consumer 使用。 |
| `conversation-semantic-projection-refactor` | **Implemented** | Question/Turn/Part/Content Node 读取合同、索引、维护审计、修复/回滚、取消协作式检查、CLI maintenance commands 已落地。历史数据库中无法逆向恢复的逐字段快照差异继续按保守策略保留。 |
| `conversation-card-contract-v1` | **Implemented** | Card contract、结构化 Content Node、适配器输出与前端投影已由 Conversation 生产链路使用。 |
| `conversation-safe-incremental-sync` | **Implemented** | 增量/全量同步、缺失与 reactivation 语义、租户隔离和共享维护路径已有测试覆盖。 |
| `skeleton-rendering-flicker` | **Implemented** | 不透明滚动表面、活动检测、共享调度器、Skeleton boundary、virtualized collection 和 Conversations 接入已落地。 |
| Tantivy 本地全文搜索计划（v1/v2） | **Implemented** | Conversation Tantivy schema、写入 lease、原子 generation、scope 查询、SQLite hydrate 和索引重建已落地。 |
| `AssetIWeave 架构统一推进` | **Implemented through work packages** | 该文件是跨域收口审计；其所列 AppService、TaskRuntime、Frontend service、Engine/CLI 和目录边界由上述计划族共同完成。 |
| `Memory 页面与渐进式回忆实施计划` | **Superseded** | 旧 Dream/Library/Evidence 语义由 `memory-rewrite` 替代；旧 UI、旧 API、旧后台触发路径已退出，历史 migration 仅用于升级/归档。 |
| `后端里程碑审计建议` | **Governance reference** | 作为审计规则和质量门保留，当前结论以本索引及各计划的最新证据为准。 |

## 3. 当前交付内容

### Memory 重写

- Session → Project → Global 的 durable SQLite workflow，包含 revision、watermark、lease、
  retry、heartbeat、失效传播和 app-owned Markdown 原子发布。
- `memory.recent.list`、`memory.context.resolve`、`memory.project.get`、
  `memory.rebuild`、`memory.task.*`、`memory.recall.*` 新合同贯通 Tauri、Engine、CLI、
  Frontend 和内置 Skill。
- Recall 使用持久 Session/Turn，结构化输出只允许 `answer`、`sessionReferences`、
  `contentReferences`、`followUpSuggestions`；引用在 tenant/session scope 内验证。
- Context/Recall 实际采用内容才写 usage；生成/使用开关、排除规则和四类 action assignment
  持久化到设置系统。
- Memory 页面只保留「近期」「回忆」；全局 task provider 使用 event + polling，取消和重试
  不阻塞无关导航/筛选操作。
- 旧 Memory 数据只生成一次 app-owned 只读归档；历史 migration 不改写，新查询不读取旧表。

### 跨计划收口

- Conversation 维护任务具备 audit/repair/rollback 的 Go CLI 专用入口，并把取消 token
  传入 sync、audit、repair、reindex 和验证阶段。
- Content Node 生产读取要求 canonical `projected_content_nodes`；旧 blocks fallback
  仅保留测试 fixture/历史兼容读取，不再是生产展示事实源。
- Agent Market release evidence 的 catalog hash 已与当前 catalog 对齐，网络 release 和真实
  ACP binary smoke 可重放。

## 4. 可复现验证

以下命令在 2026-09-07 当前提交通过：

| 层级 | 命令 | 结果 |
|---|---|---|
| Rust | `cargo fmt --all -- --check` | PASS |
| Rust | `cargo test --workspace` | PASS：905 tests |
| Frontend | `pnpm typecheck` | PASS |
| Frontend | `pnpm lint --quiet` | PASS |
| Frontend | `pnpm format:check` | PASS |
| Frontend | `pnpm test` | PASS：142 files / 699 tests |
| Frontend | `pnpm build` | PASS |
| Go | `go vet -C cli ./...` | PASS |
| Go | `go test -C cli -race ./...` | PASS |
| CLI E2E | `go test -C cli -tags=e2e -count=1 ./tests/cli_e2e` | PASS：66.234s |
| 边界 | `pnpm check:boundaries` | PASS |
| Contract | `pnpm cli:contract && git diff --exit-code -- cli/internal/schema/contract.json` | PASS：0 contract drift |
| Surface | `pnpm gen:surface-matrix && pnpm check:surface-matrix` | PASS：42 explicit exemptions |
| 安全 | `cargo audit` | PASS：0 vulnerabilities (21 allowed warnings) |

## 5. 有意保留的历史项

1. 已发布 migration 不删除、不改写；旧 Memory 表名和旧设置键只在 migration、只读归档
   或迁移兼容读取中出现。
2. 旧数据库中已经丢失的逐字段 Question snapshot 差异不伪造恢复；当前 Conversation
   Turn/Part 事实和保守计数继续保留。
3. 真实桌面视觉与关闭流程仍属于发布前人工 smoke；自动化测试覆盖其状态、取消、路由、
   provider 和任务投影契约，不用自动化结果替代视觉检查。

## 6. 工作区保护

以下既有用户修改未纳入本次提交，仍保持原样：

```text
AGENTS.md
CONTEXT.md
src-tauri/src/adapters/cli_tools.rs
src-tauri/src/adapters/platform.rs
src-tauri/src/backend/host_process.rs
src-tauri/src/backend/logs.rs
src-tauri/src/backend/path_utils.rs
```
