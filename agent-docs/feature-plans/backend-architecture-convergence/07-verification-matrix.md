# SPEC-BA-07：验收证据矩阵与完成门禁

- 状态：Proposed v1
- 目的：防止“类型存在/测试绿色”被误判为架构迁移完成
- 原则：每项必须有与要求同等范围的直接证据

## 1. 证据等级

| 等级 | 证据 | 可证明内容 |
|---|---|---|
| E1 | 文本搜索、编译器 warning | 线索；不能证明行为完成 |
| E2 | 单元测试 | 局部算法/类型行为 |
| E3 | 跨模块集成测试 | workflow、DB、filesystem、runtime 交互 |
| E4 | Tauri/Engine contract 测试 | surface 一致性 |
| E5 | 本地端到端/发布 evidence | 用户场景与真实 distribution |

P0 用户功能至少需要 E4；真实 Agent 安装恢复至少需要 E5。

## 2. Requirement Traceability Matrix

| ID | Requirement | 必需证据 | 失败判定 |
|---|---|---|---|
| ERR-01 | Application 只用 runtime AppResult | E1 + full compile | `dto::AppResult` 或 String alias 仍被引用 |
| ERR-02 | Tauri/Engine 同错误码 | E4 | 同一输入 code/retryable 不同 |
| ERR-03 | Legacy 单调退出 | 全量零引用搜索 + guard self-test | 任一生产位置出现 LegacyResult |
| LAY-01 | Runtime 不依赖 Application | E1 + guard self-test | runtime 命中 application import |
| TASK-01 | TaskRuntime 唯一 lifecycle | E2/E3 | projection 可独立返回 orphan Running |
| TASK-02 | Scan 后台化 | E3 + UI test | Tauri command 等待完整 scan |
| TASK-03 | Batch mount 后台化 | E3 + UI test | 无取消/进度/冲突键 |
| TASK-04 | Engine 复用 workflow | E4 | Engine/Tauri 各自实现业务逻辑 |
| PROC-01 | Probe 统一 HostProcess | E1 + E3 | domain 仍有裸 Command runner |
| PROC-02 | timeout/cancel 清理树 | E3 | child/grandchild 任一残留 |
| PROC-03 | 输出有界无死锁 | E3 | cap 后阻塞或把截断当成功 |
| EXT-01 | Kernel 有生产执行者 | E1 + E3 | ProbeResult/Launcher 仍 dead code |
| EXT-02 | Hook 有明确语义 | E1 + E2 | 两领域继续空实现 hook |
| CAT-01 | ACP core/version 仅作观测 | E2 + release gate | core/version metadata 阻断 lifecycle |
| CAT-02 | Catalog 按 revision/content 选择 | E3 | core range 参与 active selection |
| CAT-03 | production Catalog 无 fixture | release gate | fixture/example/placeholder 命中 |
| CAT-04 | UI/contract 不再暴露 core compatibility 门禁语义 | frontend/contract test | 版本观察字段重新阻断 install/update/reinstall |
| CAT-05 | Agent update 原子切换 | E3/E5 | 失败更新破坏旧安装 |
| CAT-06 | 至少一条真实 ACP 安装证据 | E5 | 只有 fake fixture test |
| TGT-01 | Runtime TargetCatalog 驱动 defaults | E3 | defaults 内部 builtin |
| TGT-02 | Provider-neutral mount | E3 | fixture provider 需要改 AppKind |
| ACT-01 | canonical assignments v3 | E2/E3 | resolver 仍读 legacy key |
| ACT-02 | Action 不误回退 | E2 | prompt/memory 回退 card translation |
| ACT-03 | legacy executor 删除 | E1 + compile | 任一旧符号存在 |
| EVT-01 | Domain event 与业务同事务 | E3 | commit 后补写或只靠 Tauri event |
| EVT-02 | ResidentHost 唯一 dispatcher | E3 | OneShot 启动 dispatcher |
| EVT-03 | Consumer 先提交效果再推进 offset | E3 | 入队即返回 Ok 或 offset 先推进 |
| IFACE-01 | Engine registry 保持元数据 Authority | E4 | 新建第三份 risk/exposure 表 |
| IFACE-02 | surface matrix 与实现一致 | 生成检查 + 抽查 | 生成物 stale 或遗漏 Tauri command |
| BAK-01 | Backup 独立 Runtime 仅作明确特例 | E2 + E3 | 普通 workflow 复制独立 pool 模式 |
| GATE-01 | 守卫可阻止回潮 | guard self-test | 只测试当前代码不测试违规 fixture |
| DOC-01 | 文档与真实状态一致 | 完成审计 | 未满足项被标 `[X]` |

## 3. 必跑自动化命令

### 3.1 每个 Rust 后端提交

```bash
cargo fmt --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm check:boundaries
```

### 3.2 Frontend/DTO 变更

```bash
pnpm typecheck
pnpm test
pnpm build
```

### 3.3 Engine/CLI surface 变更

```bash
pnpm cli:contract
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
git diff --exit-code -- cli/internal/schema/contract.json agent-docs/generated/surface-matrix.md
```

最后一条在生成物应提交时应改为检查预期 diff 已纳入提交；不得手工修改生成内容。

### 3.4 Agent Catalog 变更

```bash
node --test scripts/check-agent-catalog-release.test.mjs
node scripts/check-agent-catalog-release.mjs --static
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market
pnpm test -- AgentSettingsPanel
```

发布前额外运行：

```bash
node scripts/check-agent-catalog-release.mjs --release
cargo test --manifest-path src-tauri/Cargo.toml agent_market_lifecycle_e2e
```

## 4. 必须为零的搜索

完成整个里程碑后：

```bash
rg 'type AppResult<T> = Result<T, String>' src-tauri/src/backend
rg 'dto::.*AppResult|dto::\{[^}]*AppResult' src-tauri/src/backend/application
rg 'backend::application|crate::backend::application' src-tauri/src/backend/runtime
rg 'legacy_gemini|configured_agent_capability|AiCliRuntime|AiStructuredTextRequest|execute_structured_text|run_cli_command' src-tauri/src
rg 'Command::new|tokio::process::Command' \
  src-tauri/src/backend/application \
  src-tauri/src/backend/conversations \
  src-tauri/src/backend/ai_execution/backends
rg 'TargetCatalog::builtin\(' \
  src-tauri/src/backend/app_paths.rs \
  src-tauri/src/backend/defaults.rs \
  src-tauri/src/backend/application
rg 'fixture|example\.com' builtin-assets/agent-market
```

测试 fixture 目录可通过精确 exclude 排除；不得对整个领域目录使用 `|| true`。

## 5. 手工场景验收

### M-01：旧缓存升级

前置：放入合法但仅支持 `<0.6.0` 的 cache。

1. 启动当前 `0.6.1` App。
2. 打开 Agent Market。
3. 观察 active catalog origin/version。
4. 点击 compatible ACP item 安装。

通过：旧 cache 被标 inactive，compatible bundled/remote 被选中，preview 正常打开。

### M-02：不兼容条目

1. 加载含兼容与不兼容 item 的 catalog。
2. 打开 market。

通过：不同观察版本的 item 可见；仅版本差异不禁用 install/update/reinstall，制品完整性、平台适配和 ACP conformance 仍按门禁执行；已安装条目可卸载。

### M-03：安装成功

1. Preview managed distribution。
2. 确认。
3. 离开页面再返回。
4. 完成后测试连接、选择模型。

通过：全局进度持续可见；terminal 后状态 Ready；重启仍恢复。

### M-04：更新失败回滚

1. 安装 v1。
2. 让 v2 conformance fixture 失败。
3. 执行 update。

通过：任务 Failed；v1 仍 active/Ready；staging 清理；没有半写 DB row。

### M-05：扫描与批量挂载响应性

1. 启动慢 scan/batch fixture。
2. 同时导航、筛选、打开设置。
3. 取消任务。

通过：无关 UI 可用；进度更新；取消后不再执行新 item；关闭提示正确。

## 6. 性能和资源门槛

- Tauri start task command：测试 fixture 下 P95 < 200ms 返回 snapshot。
- Task polling：默认不得快于 250ms；事件正常时 SHOULD 降低轮询频率。
- Probe 默认 deadline 必须明确且有上界；禁止 `output().await` 无 deadline。
- stdout/stderr cap 必须为常量或 request limits，不得无限增长。
- batch workflow 对 N 个 item 不得执行 N 次全 catalog refresh。

性能测试允许在发布 checklist 单独运行，但实现必须有可测量 instrumentation。

## 7. 编译器 warning 审计

最终 Rust workspace test 报告 31 条 lib warning、35 条 lib-test warning，主要是迁移范围外的既有无效括号、未使用 re-export 和测试辅助方法。完成时：

- `legacy_gemini`、`configured_agent_capability`、Kernel `ProbeResult`、AppRuntime
  `target_catalog` 的 dead-code warning MUST 消失。
- 新增代码 MUST 不产生 warning。
- 若仍有无关 warning，必须列出精确 allowlist、owner 和移除 task；不得用全局
  `#![allow(dead_code)]`。

## 8. 完成审计模板

```markdown
### Requirement: CAT-02
- Status: achieved | incomplete | contradicted | missing evidence
- Evidence:
  - test: ...
  - source: ...
  - command output: ...
- Negative search: ...
- Remaining risk: ...
```

所有表项完成后才能更新 GitHub Issues（已取代文件版任务总册） 为 `[X]`。任何 `incomplete/contradicted/missing`
都意味着整个收口里程碑仍未完成。

## 9. 2026-08-24 二次收口施工证据

本节只记录本轮实际修改并通过自动化验证的条目，不代表整份矩阵的所有发布级手工场景均已完成。

| Requirement | Status | 直接证据 |
|---|---|---|
| 租户运行时资源一致性 | achieved | `switching_tenant_rebinds_tenant_scoped_runtime_catalogs` 证明切换后会话 Adapter Catalog 随完整快照切换；Agent runtime manager/runtime 与请求上下文一同发布 |
| ACT-01 | achieved | SQLite 行存在时不再读取或写回旧 `config.json`；前端 `AppSettings` 正常模型只保留 `agentAssignments`；旧字段只进入一次性迁移函数 |
| TASK-02 | achieved | `CatalogTaskProvider` 成为 source scan 唯一订阅/轮询者；页面仅记录自己启动的 task id 并在终态刷新一次 |
| TASK-03 | achieved | explicit/group/exclusive 均通过后台 BatchMount task；终态前不刷新、不显示成功；立即返回 terminal snapshot 的竞态有回归测试 |
| TASK-04 | achieved | Tauri worker 与 Engine 兼容入口都委托 `run_batch_mount_workflow_with_progress` |
| ERR-02 | achieved | AI model-selection/protocol detail 与 Extension Kernel 错误保留 `code`、`retryable` 和安全 message |
| ERR-03 | achieved | `rg` 证明 `src-tauri/src` 中 `LegacyResult` 引用为 0；边界脚本直接拒绝新增引用，已删除 LegacyResult allowlist |
| TGT-01/TGT-02 | achieved | 生产启动从 app-owned `target-providers/*.json` 构造校验后的 catalog；Tauri/Engine 提供 list/refresh；无效集合在发布前失败，既有快照不被替换 |

自动化结果：

- `cargo fmt --all -- --check` passed。
- `cargo test --workspace`：677 passed，1 ignored，0 failed。
- `pnpm typecheck`、`pnpm test`：109 files / 553 tests passed；`pnpm build` passed。
- `go vet -C cli ./...`、`go test -C cli -race ./...`、`pnpm cli:test:e2e` passed。
- `pnpm cli:contract`、`pnpm check:surface-matrix`、`scripts/check-module-boundaries.sh`、边界 self-test passed。
- `node scripts/check-agent-catalog-release.mjs --static` 与 `--release` 均 passed。
- `rg 'LegacyResult' src-tauri/src` 返回 0；Legacy error allowlist 已删除。

尚未在本节声明完成的证据：M-01 至 M-05 桌面手工场景、release/network Agent catalog gate、100k Recall ignored 性能 fixture。它们继续保留在发布 checklist，不影响上述代码缺陷已收口的判断。

## 10. Issue #2 最终对照

| 领域 | 落地内容 | 直接证据 |
|---|---|---|
| Settings/tenant authority | SQLite row 是运行时唯一设置源；旧 JSON 仅在 row 缺失时迁移一次；租户切换在发布新快照前重载 Agent Registry | `sqlite_settings_import_is_idempotent_and_legacy_keys_are_removed`、`sqlite_settings_remain_available_when_legacy_file_is_corrupt`、`switching_tenant_rebinds_tenant_scoped_runtime_catalogs` |
| AI/ACP | execution phase、failure phase、cleanup report 分离；cancel/timeout 有界收敛；native 输出、退出码、读取错误和权限事件统一失败闭合 | `tauri_03_04_failure_keeps_execution_phase_separate_from_cleanup_phase`、ACP `life_*`/timeout/cancel tests、native backend tests |
| HostProcess | 进程组清理、后代管道占用和 bounded output drain 均有期限 | `normal_exit_reaps_descendants_before_joining_output_readers`、`recorded_group_is_cleaned_after_the_launcher_exits`、host process timeout tests |
| Agent lifecycle | 卸载前校验 assignment，先保存清理状态；生命周期失败补偿原始 settings；canonical normalization 保持显式未分配 | `canonical_settings_do_not_refill_explicitly_unassigned_actions`、Agent lifecycle recovery e2e、frontend canonical-assignment regression |
| Remote Skill | acquisition 走 TaskRuntime，支持 tenant dedup/progress/cancel，staging 作用域清理，导入库只记录一个正式 Asset | `acquire_skill_imports_from_isolated_git_repo_and_records_remote_source`、`remote_skill_acquire_is_tenant_scoped_and_deduplicated` |
| Task/entrypoint convergence | scan、batch mount、Remote Skill desktop entrypoints 快速返回后台任务；Engine 与 Tauri 收敛到 AppService；projection decode 错误不再伪装为 not found | boundary/surface checks、`registry_matches_tauri_handler_methods`、`optional_projection_getters_surface_decode_errors_instead_of_not_found` |
| Error/shutdown | AppError/WireError 保留 code/retryable/safe details；公开错误清理基础设施诊断；shutdown 先停新任务再等待、drain dispatcher、关闭 DB | runtime error redaction tests、`tauri_08_app_close_wait_is_bounded_and_reports_pending_cleanup`、runtime shutdown tests |
| TargetCatalog/version contract | 多 rule、最长路径和同具体度歧义校验；刷新协调既有租户；Agent core/version 不再作为生命周期门禁，公开兼容字段已删除 | target catalog/path/reconciliation tests、catalog preview version tests、CLI contract regeneration |
