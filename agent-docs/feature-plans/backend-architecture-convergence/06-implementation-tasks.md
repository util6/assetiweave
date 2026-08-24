# SPEC-BA-06：分阶段实施任务清单

- 状态：Proposed v1
- 排序原则：先恢复用户功能，再消除双 Authority，最后清理兼容层
- 执行单位：每个任务一个聚焦提交；默认不超过 5 个实现文件（测试文件计入）
- 提交消息：中文 Conventional Commit，例如 `fix: 修复 Agent 目录核心版本兼容判断`

## 1. 依赖图

```text
BA-001 ─→ BA-002 ─→ BA-003 ─→ BA-004 ─→ BA-005

BA-006 ─→ BA-007 ─→ BA-008

BA-009 ─→ BA-010 ─→ BA-011
                    └→ BA-012

BA-013 ─→ BA-014 ─→ BA-015

BA-016 ─→ BA-017

BA-018 ─→ BA-019 ─→ BA-020

全部实现 ─→ BA-021 ─→ BA-022
```

BA-001 至 BA-005 是当前 Agent Market 故障恢复链，MUST 最先完成。其他分支在不修改同一文件
时 MAY 并行。

## 2. Phase A：Agent Market P0 恢复

### BA-001：为当前核心版本建立失败回归测试

- [ ] 在 `agent_market/catalog.rs` 增加当前 core 对 bundled item 的兼容测试。
- [ ] 把兼容判断从 Application 私有函数移动到 Catalog/Compatibility 模块，供测试复用。
- [ ] 测试在修复 catalog 前必须失败，证明捕获的是原事故。
- Acceptance：`0.6.1` 对现有 `<0.6.0` 范围明确返回 false；bundled compatibility 测试失败。
- Files：
  - `src-tauri/src/backend/agent_market/catalog.rs`
  - `src-tauri/src/backend/agent_market/mod.rs`
  - `src-tauri/src/backend/application/agent_market.rs`
- Verify：

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml bundled_catalog_items_support_current_core_version
  ```

### BA-002：发布 compatible Catalog revision

- [ ] 根据真实 contract 决定 `0.6.x` 范围，默认使用 `>=0.6.0,<0.7.0`。
- [ ] 更新 `catalogVersion/generatedAt`。
- [ ] 增加 revision parser 和 placeholder 静态检查。
- [ ] 不得只放宽代码检查；目录元数据必须正确。
- Acceptance：bundled catalog 全部条目匹配当前 core，fixture/placeholder gate 能指出现有生产数据。
- Files：
  - `builtin-assets/agent-market/catalog-v1.json`
  - `src-tauri/src/backend/agent_market/catalog.rs`
  - `scripts/check-agent-catalog-release.mjs`
  - `scripts/check-agent-catalog-release.test.mjs`
  - `package.json`
- Verify：

  ```bash
  node --test scripts/check-agent-catalog-release.test.mjs
  node scripts/check-agent-catalog-release.mjs --static
  cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::catalog
  ```

### BA-003：修复 active Catalog 与旧缓存选择

- [ ] 实现 bundled/cache candidate 比较。
- [ ] Catalog core range 仅作观测，不参与 active candidate 选择。
- [ ] 相同 revision 不同 hash 失败闭合。
- [ ] doctor/result 暴露 active origin/revision，不暴露用户绝对路径。
- Acceptance：以当前机器旧 `2026.08.16.1` cache fixture 测试时自动选择新 bundled。
- Files：
  - `src-tauri/src/backend/agent_market/cache.rs`
  - `src-tauri/src/backend/agent_market/catalog.rs`
  - `src-tauri/src/backend/application/system.rs`
  - `src-tauri/src/backend/agent_market/cache_tests.rs`（若新建）
- Verify：

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml newer_cache_is_selected_even_when_core_range_is_only_observational
  cargo test --manifest-path src-tauri/Cargo.toml same_revision_different_hash_fails_closed
  ```

### BA-004：修复前端兼容状态和操作门禁

- [ ] `AgentCatalogItem` 可保留 core compatibility 兼容字段，但只作观测。
- [ ] mapper 不丢 `coreCompatible`。
- [ ] 仅无 selectable distribution 时禁用 install/update/reinstall。
- [ ] 版本范围文案不得作为生命周期门禁。
- Acceptance：core range 不匹配仍可触发 preview；无可用分发时不可触发。
- Files：
  - `frontend/src/components/settings/agentCatalog.ts`
  - `frontend/src/components/settings/AgentConnectionRow.tsx`
  - `frontend/src/components/settings/AgentSettingsPanel.tsx`
  - `frontend/src/components/settings/AgentSettingsPanel.test.tsx`
  - `frontend/src/i18n/messages.ts`
- Verify：

  ```bash
  pnpm typecheck
  pnpm test -- AgentSettingsPanel
  ```

### BA-005：生产 Catalog 与安装链端到端

- [ ] 把 fixture catalog 移到测试资源。
- [ ] production catalog 替换真实 package/artifact/evidence。
- [ ] 使用本地 fake ACP executable 验证 install/update/rollback/restart recovery。
- [ ] release job 验证真实 distribution 元数据，不在普通 unit test 下载公网包。
- Acceptance：至少一个 ACP managed distribution 有可追溯 evidence；本地 e2e 全路径通过。
- Files：应先由执行者列出精确拆分；若超过 5 个文件，必须拆成“Catalog production 化”和
  “Lifecycle e2e”两个提交。
- Verify：

  ```bash
  node scripts/check-agent-catalog-release.mjs --release
  cargo test --manifest-path src-tauri/Cargo.toml agent_market_lifecycle_e2e
  ```

## 3. Phase B：AppError 与分层

### BA-006：建立 WireError 和 typed conversion

- [ ] 增加 `WireError` 与稳定 code/retryable 映射。
- [ ] Agent Market typed error 禁止 `to_string()` 往返。
- [ ] Tauri/Engine 同一错误结果一致。
- Acceptance：validation/not-found/conflict/cancel/timeout 均有稳定 wire code。
- Files：
  - `src-tauri/src/backend/runtime/error.rs`
  - `src-tauri/src/backend/dto/error.rs`
  - `src-tauri/src/adapters/tauri/error.rs`
  - `src-tauri/src/adapters/engine/error.rs`
  - 对应测试文件
- Verify：运行目标 error tests 与 `pnpm cli:contract`。

### BA-007：按 Agent Market 垂直切片迁移 AppResult

- [ ] Application Agent Market 返回 runtime AppResult。
- [ ] lifecycle/catalog/cache errors 有 typed conversion。
- [ ] 前端错误显示只读 code/message，不解析 Display 字符串。
- Acceptance：该切片无 `map_err(|e| e.to_string())` 退化点。
- Files：
  - `src-tauri/src/backend/application/agent_market.rs`
  - `src-tauri/src/backend/agent_market/types.rs`
  - `src-tauri/src/backend/agent_market/lifecycle/mod.rs`
  - `frontend/src/services/agentRuntime.ts`
  - 测试文件

### BA-008：删除 DTO AppResult 并完成 Application 迁移

- [ ] 分 PR 迁移剩余领域；每个 PR 只迁移一个领域。
- [ ] 最后删除 `dto::AppResult`，prelude 改导 runtime AppResult。
- [ ] 建立 Legacy 精确 allowlist。
- Acceptance：Application 无 DTO AppResult、显式 String error result 或新增 Legacy。
- Files：每个领域 PR 必须在开始前列出，单 PR 不超过 5 个实现/测试文件。
- Verify：`pnpm check:boundaries`、完整 Rust tests。

### BA-009：消除 Runtime → Application 依赖

- [ ] 新建中立 bootstrap 模块。
- [ ] 移动 prepared builtin adapter materialization/seed orchestration。
- [ ] Runtime 只调用 bootstrap/store。
- Acceptance：`rg 'backend::application' src-tauri/src/backend/runtime` 返回 0。
- Files：
  - `src-tauri/src/backend/bootstrap/mod.rs`
  - `src-tauri/src/backend/bootstrap/startup.rs`
  - `src-tauri/src/backend/application/bootstrap.rs`
  - `src-tauri/src/backend/runtime/app_runtime.rs`
  - `src-tauri/src/backend/mod.rs`

## 4. Phase C：TaskRuntime 与长任务

### BA-010：把 Registry 改为纯 Projection

- [ ] getter 组合 TaskRuntime snapshot 与 domain data。
- [ ] 删除 domain snapshot 中独立 terminal/running authority。
- [ ] runtime remove/cancel/retention 后 list/get 一致。
- Acceptance：故意保留 orphan projection 时 API 不返回 Running。
- Files：
  - `src-tauri/src/adapters/tauri/background_tasks.rs`
  - `src-tauri/src/backend/runtime/tasks.rs`
  - 相关 DTO 文件
  - 测试文件

### BA-011：Source scan 后台化

- [ ] 提取共享 SourceScanWorkflow。
- [ ] ResidentHost start/get/list/cancel commands。
- [ ] Engine 同步调用同一 workflow。
- [ ] 前端 provider + 全局进度接入。
- Acceptance：start 快速返回；重复请求去重；导航仍可用。
- Files：必须拆成 backend workflow、Tauri adapter、frontend integration 三个提交。
- Verify：目标 Rust tests、frontend tests、`pnpm check:surface-matrix`。

### BA-012：Batch mount 后台化

- [ ] group/exclusive/explicit mount 统一 workflow。
- [ ] 一次加载共享数据、一次最终 refresh。
- [ ] 定义 partial failure 与 compensation。
- [ ] 前端展示 item error 与总体结果。
- Acceptance：取消不会执行下一个物理动作；同 profile 冲突被 TaskRuntime 拦截。
- Files：按 capability/workflow、adapter、frontend 三个提交拆分。

## 5. Phase D：HostProcess 与 Extension Kernel

### BA-013：实现统一 async HostCommand runner

- [ ] async + blocking facade 共享内核。
- [ ] timeout/cancel 杀进程树并 reap。
- [ ] bounded stdout/stderr drain。
- Acceptance：child/grandchild、large output、invalid UTF-8 fixture 全通过。
- Files：
  - `src-tauri/src/backend/host_process.rs`
  - `src-tauri/src/backend/host_process/tests.rs`（若拆分）
  - `src-tauri/Cargo.toml`（仅确有依赖时）

### BA-014：迁移 conversation process/probe

- [ ] external adapter invoke 使用 HostProcess。
- [ ] runtime version probe 使用 Kernel launcher。
- [ ] 删除私有 polling/kill/read_capped runner。
- Acceptance：现有 conversation fixture 行为不变，错误码统一。
- Files：
  - `src-tauri/src/backend/conversations/external.rs`
  - `src-tauri/src/backend/conversations/io_utils.rs`
  - `src-tauri/src/backend/extension_kernel/launcher.rs`
  - conversation tests

### BA-015：迁移 Agent native probe/model discovery

- [ ] connection probe/model discovery 使用共享 deadline/cap/cancellation。
- [ ] 删除裸 `output().await`。
- [ ] model parse 保持纯函数。
- Acceptance：永不退出 fixture 在 deadline 内稳定 Timeout，并清理进程树。
- Files：
  - `src-tauri/src/backend/ai_execution/backends/native.rs`
  - `src-tauri/src/backend/extension_kernel/launcher.rs`
  - `src-tauri/src/backend/ai_execution/error.rs`
  - 测试文件

### BA-016：收口 DomainPackageSystem

- [ ] 默认删除空 `on_installed/on_removed` hook。
- [ ] 更新 0011 和 module docs，确保职责与代码一致。
- [ ] 增加 kernel production-use test。
- Acceptance：无 dead `ProbeResult`、空 hook 或未构造 ExtensionError 分支。
- Files：
  - `src-tauri/src/backend/extension_kernel/mod.rs`
  - `src-tauri/src/backend/conversations/package.rs`
  - `src-tauri/src/backend/agent_market/runtime.rs`
  - `agent-docs/adr/0011-extension-kernel-shared-primitives.md`
  - 测试文件

## 6. Phase E：Target 与 Agent Capability

### BA-017：TargetCatalog 接管 defaults/profile

- [ ] defaults 显式接收 runtime catalog。
- [ ] AppPathCatalog 不再内部 builtin。
- [ ] seed 保留用户配置。
- Acceptance：fixture provider 自动生成默认 Profile。
- Files：
  - `src-tauri/src/backend/defaults.rs`
  - `src-tauri/src/backend/app_paths.rs`
  - `src-tauri/src/backend/runtime/app_runtime.rs`
  - `src-tauri/src/backend/application/bootstrap.rs` 或新 bootstrap
  - 测试文件

### BA-018：TargetCatalog 接管 detection/planner/mount

- [ ] 删除 path_utils 硬编码 target 表。
- [ ] workflow 按 provider ID 查询 descriptor。
- [ ] 新 provider symlink mount e2e。
- Acceptance：无 AppKind/Rust 修改即可 mount fixture provider。
- Files：按 detection、planner、mount e2e 分至少两个提交。

### BA-019：Settings v3 canonical assignments

- [ ] 实现 action registry 和 canonical settings schema。
- [ ] 实现幂等 legacy migration。
- [ ] resolver 不再 fallback 到 cardTranslation/aiRuntime。
- Acceptance：显式值保留、memory 正确 fan-out、未知 action 失败闭合。
- Files：
  - `src-tauri/src/backend/ai_execution/composition.rs`
  - `src-tauri/src/backend/app_settings.rs`
  - `frontend/src/store/settings/settingsSchema.ts`
  - 后端测试
  - 前端测试

### BA-020：删除 legacy Agent executor

- [ ] compatibility API 全部委托 AgentExecutionRuntime。
- [ ] 删除 `legacy_gemini` 与 structured CLI executor。
- [ ] 删除对应旧测试，改为 injected runtime 行为测试。
- Acceptance：SPEC-BA-05 §7 符号搜索全部为 0。
- Files：
  - `src-tauri/src/backend/ai_execution/mod.rs`
  - `src-tauri/src/backend/ai_execution/legacy_gemini.rs`（删除）
  - `src-tauri/src/backend/card_translation.rs`
  - `src-tauri/src/backend/application/card_translation.rs`
  - 测试文件

## 7. Phase F：守卫、文档与完成审计

### BA-021：架构守卫升级

- [ ] 从全局计数改为目录基线 + 精确 allowlist。
- [ ] 增加 Command、AppResult、runtime dependency、legacy Agent、TargetCatalog 旁路检查。
- [ ] 每条守卫有 self-test：插入违规 fixture 时必须失败。
- Acceptance：不能通过改名/re-export/删除一处新增一处绕过。
- Files：
  - `scripts/check-module-boundaries.sh`
  - `scripts/check-module-boundaries.test.sh`
  - `.github/workflows/ci.yml`
  - `package.json`

### BA-022：规范、任务状态与生成契约同步

- [ ] 逐条完成 SPEC-BA-07 证据矩阵。
- [ ] 更新 GitHub Issues（已取代文件版任务总册），不得先标完成。
- [ ] 更新 已淘汰的全局设计总册（以代码、测试与 ADR 为准） 和 repository structure。
- [ ] 若 Engine contract 变化，重新生成 contract/surface matrix。
- Acceptance：每个 MUST 有直接测试、搜索、运行或文档证据；无“推断完成”。
- Verify：SPEC-BA-07 全命令。

## 8. 每个任务的提交要求

每个提交描述必须包含：

```text
Task ID:
Canonical authority changed:
Legacy path removed/delegated:
Behavior change:
Tests added:
Commands run:
Known remaining seam:
Rollback:
```

如果一个提交只新增新抽象而没有迁移至少一个生产 consumer，视为未完成，不得单独标记任务完成。
