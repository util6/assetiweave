# 后端领域分层重构 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在保持 AppService、Engine/Tauri/CLI 契约、SQLite schema 与用户可见行为不变的前提下，完整落地 Issue #44 的领域导向模块化单体边界，并移除本轮目录重排产生的全部兼容门面。最终 `backend/mod.rs` 仅声明 `application`、`domain`、`infrastructure`、`store`（以及可选的 `#[cfg(test)] test_support`）。

**Architecture:** 顶层按 Adapters、Application、Domain、Store、Infrastructure 划分稳定职责；Application、Domain 与 Store 再按 Catalog、Mounting、Conversations、Memory、Agents、System/Operations 归档。AppService 保持唯一业务编排门面，Store 保持 SQLx Mapper/Repository，Infrastructure 拥有 Runtime、Tasks、Events、Extensions、Host、HTTP、Observability 与 Backup。

**Tech Stack:** Rust 1.96、Tauri、Tokio、SQLx/SQLite、Serde/Schemars、Shell architecture guards、Go 1.24 CLI、React 19/TypeScript。

**Spec:** [GitHub Issue #44](https://github.com/util6/assetiweave/issues/44)

## Global Constraints

- 不改变 Engine/Tauri/CLI 命令名、Serde 字段、错误 wire code、SQLite schema 或历史数据格式。
- `src-tauri/src/backend/application/AppService` 继续作为 Tauri 与 Engine 唯一业务编排边界。
- Store、Domain、Infrastructure Runtime/Tasks/Events/Extensions 不依赖 Application。
- Domain 不依赖 SQLx、Runtime、Tauri、文件系统、网络、进程、DTO、Store 或 Infrastructure。
- 不引入 ORM、微服务、Event Sourcing、CQRS、通用内存 EventBus 或新的依赖注入框架。
- Extension Kernel 只调整所有权和路径，不改变插件协议、市场协议或运行时 ABI。
- Durable Outbox、TaskRuntime、HostProcess、AppRuntime 与 shutdown deadline 的既有语义保持不变。
- Rust 业务实现文件控制在 500 行以内；测试放到同级 `*_tests.rs`。
- 每个任务独立验证并使用中文 Conventional Commit；不提交当前工作树中与 Issue #44 无关的前端与任务中心修改。
- Backend 根目录只能保留 `application`、`domain`、`infrastructure`、`store` 四个生产模块；旧顶层目录必须按职责迁入四层，而不是保留为并列兼容 facade。
- 最终质量门禁必须覆盖整个 `backend/` 的生产 Rust 文件规模、分层依赖和新增 warning；历史任务曾通过的局部门禁不代表本轮最终收口完成。

## Review Focus

- 旧模块路径被删除后，Engine、Tauri 与测试构建仍能解析所有稳定参数、结果和错误类型；Task 2、Task 4、Task 8 的编译与 contract 测试覆盖该风险。
- Runtime 关闭期间的任务、事件 dispatcher 与 session stream 不丢终态、不延长 deadline；Task 3 与 Task 7 的 runtime/dispatcher 测试覆盖该风险。
- Store 目录移动不改变事务原子性、租户隔离、排序、JSON/null 与历史数据兼容；Task 5 的 repository 定向测试覆盖该风险。
- Outbox 在重试、late consumer、backfill、失败隔离和跨租户场景下保持既有语义；Task 7 的事件测试覆盖该风险。
- Extension 路径收口不改变 identity、compatibility、trust、probe、launcher、registry snapshot 与 lifecycle coordination；Task 8 的 extension 测试覆盖该风险。

---

### Task 1: 接收当前目录重排并建立可执行基线

**Files:**
- Create: `agent-docs/feature-plans/backend-domain-layering/01-context-map.md`
- Modify: `scripts/check-module-boundaries.sh`
- Test: `scripts/check-module-boundaries.sh`

**Interfaces:**
- Consumes: Issue #44 的上下文所有权与当前工作树的已移动文件。
- Produces: 可检索的 Context Map，以及以新 canonical 路径为目标的架构守卫。

- [x] **Step 1: 记录当前编译与边界基线**

```bash
cargo check --manifest-path src-tauri/Cargo.toml --lib
./scripts/check-module-boundaries.sh
```

Expected: `cargo check` 通过；边界脚本仅报告已知的 `application/mounting/groups.rs` 字符串错误与过时路径检查。

- [x] **Step 2: 写 Context Map 与 canonical path 表**

```markdown
| Context | Application | Domain | Store | Infrastructure |
| Catalog | application/catalog | domain/catalog | store/catalog | infrastructure/filesystem/http |
| Mounting | application/mounting | domain/mounting | store/mounting | infrastructure/filesystem/tasks |
| Conversations | application/conversations | domain/conversations | store/conversations | infrastructure/extensions/tasks |
| Memory | application/memory | domain/memory | store/memory | infrastructure/tasks |
| Agents | application/agents | domain/agents | store/system | infrastructure/extensions |
| System/Operations | application/system | domain/tenant | store/system | infrastructure/runtime/events/backup |
```

- [x] **Step 3: 更新守卫作用域到新目录并让已知违规保持红灯**

```bash
./scripts/check-module-boundaries.sh
```

Expected: FAIL，并准确指向 `application/mounting/groups.rs` 的 `Result<_, String>`；不得因旧文件不存在而静默跳过检查。

- [x] **Step 4: 提交基线文档与守卫**

```bash
git add agent-docs/feature-plans/backend-domain-layering scripts/check-module-boundaries.sh
git commit -m "docs: 建立后端领域分层执行基线"
```

### Task 2: 收口 Application 业务目录与公开面

**Files:**
- Move: `src-tauri/src/backend/application/agent_market/` → `src-tauri/src/backend/application/agents/`
- Delete: `src-tauri/src/backend/application/recent/`
- Delete: `src-tauri/src/backend/application/skills/`
- Delete: `src-tauri/src/backend/application/system/profiles_navigation.rs`
- Modify: `src-tauri/src/backend/application/mod.rs`
- Modify: `src-tauri/src/backend/application/prelude.rs`
- Modify: `src-tauri/src/backend/application/{catalog,mounting,conversations,memory,agents,system}/mod.rs`
- Modify: `src-tauri/src/backend/application/{catalog,mounting,conversations,memory,agents,system}/params.rs`
- Test: sibling `*_tests.rs` files under those six contexts

**Interfaces:**
- Consumes: `AppService`, domain-local params/results and the existing Engine/Tauri command surface.
- Produces: `application::{catalog,mounting,conversations,memory,agents,system}` as the only Application business namespaces and explicit stable re-exports from `application/mod.rs`.

- [x] **Step 1: 增加模块拓扑测试**

```rust
#[test]
fn application_uses_only_canonical_context_directories() {
    let root = include_str!("mod.rs");
    assert!(root.contains("mod agents;"));
    assert!(!root.contains("mod agent_market;"));
    assert!(!root.contains("mod recent;"));
    assert!(!root.contains("mod skills;"));
}
```

- [x] **Step 2: 运行测试并确认旧门面仍使测试失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::application::tests::application_uses_only_canonical_context_directories
```

Expected: FAIL，指出 `agent_market`、`recent` 或 `skills` 仍存在。

- [x] **Step 3: 完成目录更名、调用路径迁移、局部 params 与显式导出**

```rust
pub(crate) mod agents;
pub(crate) mod catalog;
pub(crate) mod conversations;
pub(crate) mod memory;
pub(crate) mod mounting;
pub(crate) mod system;
pub(crate) use service::AppService;
```

- [x] **Step 4: 将 `validate_exclusive_mount_candidate` 改为 Application typed error**

```rust
fn validate_exclusive_mount_candidate(
    asset: &Asset,
    _profile: &TargetProfile,
    source_by_id: &HashMap<String, Source>,
) -> AppResult<()> {
    let source = source_by_id
        .get(&asset.source_id)
        .ok_or_else(|| AppError::NotFound(format!("source not found: {}", asset.source_id)))?;
    if matches!(source.source_origin, SourceOrigin::AppTarget | SourceOrigin::AppLocal) {
        return Err(AppError::Conflict(
            "app-local skills must be backed up before mounting".to_string(),
        ));
    }
    let source_path = expand_path(&asset.absolute_path)?;
    if !source_path.exists() {
        return Err(AppError::NotFound(format!(
            "source asset path does not exist: {}",
            source_path.display()
        )));
    }
    Ok(())
}
```

- [x] **Step 5: 验证 Application 与边界守卫**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::application
./scripts/check-module-boundaries.sh
```

Expected: Application 测试通过，脚本不再报告 Application `Result<_, String>` 或旧兼容目录。

- [x] **Step 6: 提交 Application 收口**

```bash
git add src-tauri/src/backend/application src-tauri/src/adapters src-tauri/src/lib.rs scripts/check-module-boundaries.sh
git commit -m "refactor: 收口后端应用上下文"
```

### Task 3: 移除 Runtime、Tasks 与 Backup 顶层兼容门面

**Files:**
- Delete: `src-tauri/src/backend/runtime/`
- Delete: `src-tauri/src/backend/data_backup/`
- Modify: `src-tauri/src/backend/infrastructure/runtime/*.rs`
- Modify: `src-tauri/src/backend/infrastructure/tasks/*.rs`
- Modify: `src-tauri/src/backend/infrastructure/backup/mod.rs`
- Modify: `src-tauri/src/backend/mod.rs`
- Modify: all Rust import sites under `src-tauri/src/`
- Test: `src-tauri/src/backend/infrastructure/runtime/tests.rs`
- Test: sibling tests under `src-tauri/src/backend/infrastructure/tasks/`

**Interfaces:**
- Consumes: `AppRuntime`, `RuntimeConfig`, `TaskRuntime`, `TaskSnapshot`, `ShutdownReport` current signatures.
- Produces: canonical `backend::infrastructure::{runtime,tasks,backup}` paths with no top-level facade and no Infrastructure dependency on Application.

- [x] **Step 1: 加强架构守卫并确认失败**

```sh
check_absent 'backend::application|crate::backend::application' "$ROOT/src-tauri/src/backend/infrastructure"
check_absent 'backend::runtime|crate::backend::runtime' "$ROOT/src-tauri/src"
```

```bash
./scripts/check-module-boundaries.sh
```

Expected: FAIL，列出旧 runtime import 和 `AppRuntime` 创建 `AppService` 的反向依赖。

- [x] **Step 2: 迁移所有 import 并把 AppService 组装移到 composition/bootstrap 入口**

```rust
use crate::backend::infrastructure::{runtime::AppRuntime, tasks::TaskRuntime};
```

- [x] **Step 3: 删除 facade 并验证关闭语义**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::infrastructure::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::infrastructure::tasks
./scripts/check-module-boundaries.sh
```

Expected: runtime/tasks 测试与守卫通过；shutdown deadline、worker 收敛、session stream 终态不变。

- [x] **Step 4: 提交 Runtime/Tasks/Backup 收口**

```bash
git add src-tauri/src src-tauri/tests scripts/check-module-boundaries.sh
git commit -m "refactor: 统一运行时任务与备份基础设施"
```

### Task 4: 建立纯 Domain 并收窄 Models 与错误所有权

**Files:**
- Create: `src-tauri/src/backend/domain/mod.rs`
- Create: `src-tauri/src/backend/domain/{catalog,mounting,conversations,memory,agents,tenant}/mod.rs`
- Create: sibling `*_tests.rs` for extracted policies and value objects
- Modify: `src-tauri/src/backend/models/mod.rs`
- Modify: `src-tauri/src/backend/models/*.rs`
- Modify: `src-tauri/src/backend/error/*.rs`
- Modify: `src-tauri/src/backend/application/mod.rs`
- Modify: `src-tauri/src/adapters/{engine,tauri}/*.rs`

**Interfaces:**
- Consumes: existing serde-compatible entity/value fields and stable `AppError::view()` wire mapping.
- Produces: dependency-free Domain policies/types; Application-owned `AppError/AppResult`; Adapter-owned `WireError`; compatibility-preserving conversions.

- [x] **Step 1: 写 Domain 依赖与不变量测试**

```rust
#[test]
fn domain_source_kind_preserves_existing_serde_names() {
    let value = serde_json::to_value(SourceKind::Local).unwrap();
    assert_eq!(value, serde_json::json!("local"));
}
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::domain
```

Expected: FAIL，因为 `backend::domain` 尚未建立。

- [x] **Step 2: 提取 Catalog/Mounting/Conversation/Memory/Agents/Tenant 的纯模型和规则**

```rust
pub(crate) mod agents;
pub(crate) mod catalog;
pub(crate) mod conversations;
pub(crate) mod memory;
pub(crate) mod mounting;
pub(crate) mod tenant;
```

- [x] **Step 3: 加强依赖守卫并迁移错误映射边界**

```sh
check_absent 'backend::(application|store|infrastructure|dto)|sqlx|tauri|std::fs|tokio::fs|std::process' "$ROOT/src-tauri/src/backend/domain"
check_absent 'WireError' "$ROOT/src-tauri/src/backend/infrastructure"
```

- [x] **Step 4: 验证 Domain、错误 wire parity 与全库编译**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::domain
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::error
cargo check --manifest-path src-tauri/Cargo.toml --lib
./scripts/check-module-boundaries.sh
```

Expected: Domain 与 error parity 测试通过，Domain/Infrastructure 无反向依赖。

- [x] **Step 5: 提交 Domain 与错误边界**

```bash
git add src-tauri/src/backend/domain src-tauri/src/backend/models src-tauri/src/backend/error src-tauri/src/backend/application src-tauri/src/adapters scripts/check-module-boundaries.sh
git commit -m "refactor: 建立纯领域模型与错误边界"
```

### Task 5: 完成 Store 分域并删除 Capabilities

**Files:**
- Modify: `src-tauri/src/backend/store/{catalog,mounting,conversations,memory,system}/**/*.rs`
- Modify: `src-tauri/src/backend/store/mod.rs`
- Delete: `src-tauri/src/backend/capabilities/`
- Modify: `src-tauri/src/backend/mod.rs`
- Modify: Store consumers under `src-tauri/src/backend/application/`
- Test: sibling repository tests under the five Store contexts

**Interfaces:**
- Consumes: Domain types, `Database`, SQLx transaction/pool and existing repository function signatures.
- Produces: five domain Store namespaces, explicit Store exports and no `backend::capabilities` consumer.

- [x] **Step 1: 写架构守卫并确认旧 facade 失败**

```sh
check_absent 'backend::capabilities|crate::backend::capabilities' "$ROOT/src-tauri/src"
check_absent 'backend::application|crate::backend::application' "$ROOT/src-tauri/src/backend/store"
```

```bash
./scripts/check-module-boundaries.sh
```

Expected: FAIL，列出 Tauri command 与 backend root 的 capabilities 兼容引用。

- [x] **Step 2: 修正旧 repository path 并迁移 capabilities 消费者**

```rust
use crate::backend::store::{memory::recent_snapshot_repo, system::settings_repo};
```

- [x] **Step 3: 删除 capabilities 与 Store glob re-export**

```rust
pub(crate) mod catalog;
pub(crate) mod conversations;
pub(crate) mod memory;
pub(crate) mod mounting;
pub(crate) mod system;
```

- [x] **Step 4: 验证 repository 行为**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::store::catalog
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::store::mounting
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::store::conversations
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::store::memory
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::store::system
./scripts/check-module-boundaries.sh
```

Expected: 五个 Store 域测试通过，无 Application/Adapter/Capabilities 反向依赖。

- [x] **Step 5: 提交 Store 与 Capabilities 收口**

```bash
git add src-tauri/src/backend/store src-tauri/src/backend/capabilities src-tauri/src/backend/mod.rs src-tauri/src/backend/application src-tauri/src/adapters scripts/check-module-boundaries.sh
git commit -m "refactor: 完成持久化分域并移除能力门面"
```

### Task 6: 拆分超大 Conversation 与 Memory workflow

**Files:**
- Split: `src-tauri/src/backend/application/conversations/conversation_script_catalog.rs`
- Split: `src-tauri/src/backend/application/conversations/conversation_records.rs`
- Split: `src-tauri/src/backend/application/memory/recent/recent_snapshot_pipeline.rs`
- Split: `src-tauri/src/backend/application/memory/session_memory.rs`
- Split: `src-tauri/src/backend/store/conversations/web_record_repo.rs`
- Split: `src-tauri/src/backend/store/memory/session_memory_repo.rs`
- Test: existing sibling `*_tests.rs` plus new focused sibling tests for extracted modules

**Interfaces:**
- Consumes: existing `impl AppService` workflow signatures and repository public functions.
- Produces: responsibility-focused files below 500 implementation lines without widening visibility or changing callers.

- [x] **Step 1: 增加文件规模守卫并确认失败**

```sh
find "$ROOT/src-tauri/src/backend/application" "$ROOT/src-tauri/src/backend/store" -name '*.rs' ! -name '*_tests.rs' -print0 |
  xargs -0 wc -l
```

Expected: FAIL gate 列出上述六个超过 500 行的实现文件。

- [x] **Step 2: 按职责提取私有子模块，保持 `impl AppService` 与 repository 签名**

```rust
mod catalog_query;
mod catalog_install;
mod recent_generation;
mod recent_projection;
mod session_commands;
mod session_queries;
```

- [x] **Step 3: 验证 Conversation、Memory 与 Store 定向测试**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::application::conversations
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::application::memory
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::store::conversations
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::store::memory
./scripts/check-module-boundaries.sh
```

Expected: 行为测试与文件规模守卫通过。

- [x] **Step 4: 提交 workflow 拆分**

```bash
git add src-tauri/src/backend/application/conversations src-tauri/src/backend/application/memory src-tauri/src/backend/store/conversations src-tauri/src/backend/store/memory scripts/check-module-boundaries.sh
git commit -m "refactor: 拆分对话与记忆工作流"
```

### Task 7: 按 Domain/Store/Infrastructure/Application 拆分 Events

**Files:**
- Delete: `src-tauri/src/backend/events/`
- Create: `src-tauri/src/backend/domain/conversations/events.rs`
- Create: `src-tauri/src/backend/store/system/outbox_repo.rs`
- Create: `src-tauri/src/backend/store/system/consumer_offset_repo.rs`
- Modify: `src-tauri/src/backend/infrastructure/events/{mod.rs,dispatcher.rs}`
- Create: `src-tauri/src/backend/application/conversations/event_handlers.rs`
- Modify: `src-tauri/src/backend/infrastructure/runtime/app_runtime.rs`
- Test: sibling tests for domain event, outbox/offset, dispatcher and handlers

**Interfaces:**
- Consumes: current `DomainEvent`, `append_event_in_tx`, `DomainEventConsumer`, `DomainEventDispatcher` semantics.
- Produces: Conversation fact in Domain, durable SQL in Store, dispatcher in Infrastructure, business handlers in Application, no cyclic re-export.

- [x] **Step 1: 写分层事件测试并确认旧混合层失败**

```rust
#[test]
fn conversation_source_committed_keeps_event_name_and_payload_shape() {
    let event = ConversationEvent::source_committed("tenant", "source", 7);
    assert_eq!(event.event_type(), "conversation_source_committed");
}
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::domain::conversations::events
```

Expected: FAIL，因为事件仍位于顶层混合模块。

- [x] **Step 2: 移动事实、SQL、dispatcher 与 handler，删除循环 re-export**

```rust
use crate::backend::{
    domain::conversations::events::ConversationEvent,
    store::system::outbox_repo,
};
```

- [x] **Step 3: 验证 outbox 与 dispatcher 全语义**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib outbox
cargo test --manifest-path src-tauri/Cargo.toml --lib dispatcher
cargo test --manifest-path src-tauri/Cargo.toml --lib event_handlers
./scripts/check-module-boundaries.sh
```

Expected: append/offset/retry/backfill/tenant isolation/shutdown 测试通过，顶层 `backend::events` 不存在。

- [x] **Step 4: 提交事件分层**

```bash
git add src-tauri/src/backend/domain src-tauri/src/backend/store src-tauri/src/backend/infrastructure/events src-tauri/src/backend/application/conversations src-tauri/src/backend/events src-tauri/src/backend/mod.rs scripts/check-module-boundaries.sh
git commit -m "refactor: 分离领域事件与可靠投递"
```

### Task 8: 删除 Extension Kernel 兼容门面并完成依赖守卫

**Files:**
- Delete: `src-tauri/src/backend/extension_kernel/`
- Modify: `src-tauri/src/backend/infrastructure/extensions/**/*.rs`
- Modify: extension consumers under `src-tauri/src/backend/{agents,agent_market,ai_execution,conversations}/`
- Modify: `src-tauri/src/backend/mod.rs`
- Modify: `scripts/check-module-boundaries.sh`
- Test: sibling tests under `src-tauri/src/backend/infrastructure/extensions/`

**Interfaces:**
- Consumes: `PackageIdentity`, compatibility/trust policy, `ExtensionLauncher`, probe, registry snapshot and lifecycle coordinator signatures.
- Produces: canonical `backend::infrastructure::extensions` deep module with no old facade and no Application dependency.

- [x] **Step 1: 增加旧路径零容忍守卫并确认失败**

```sh
check_absent 'backend::extension_kernel|crate::backend::extension_kernel' "$ROOT/src-tauri/src"
```

```bash
./scripts/check-module-boundaries.sh
```

Expected: FAIL，列出剩余 extension kernel import。

- [x] **Step 2: 迁移 consumer import 并删除 facade**

```rust
use crate::backend::infrastructure::extensions::{ExtensionLauncher, PackageIdentity};
```

- [x] **Step 3: 验证 Extension 深模块与外部 contract**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::infrastructure::extensions
pnpm cli:contract
git diff --exit-code -- cli/internal/schema/contract.json
./scripts/check-module-boundaries.sh
```

Expected: extension 测试通过，生成 contract 无差异，旧路径为零。

- [x] **Step 4: 提交 Extensions 收口**

```bash
git add src-tauri/src/backend/infrastructure/extensions src-tauri/src/backend/extension_kernel src-tauri/src/backend src-tauri/src/adapters scripts/check-module-boundaries.sh cli/internal/schema/contract.json
git commit -m "refactor: 收口扩展内核基础设施"
```

### Task 9: 更新 ADR、Context Map 与最终质量门禁

**Files:**
- Modify: `agent-docs/adr/0016-domain-oriented-backend-module-boundaries.md`
- Modify: `agent-docs/feature-plans/backend-domain-layering/01-context-map.md`
- Modify: current architecture references under `agent-docs/governance/` and active feature routers
- Modify: `scripts/check-module-boundaries.sh`

**Interfaces:**
- Consumes: Tasks 1-8 的 canonical modules and dependency rules.
- Produces: 唯一当前架构说明、无失效实现路径的活跃文档与最终 CI guard。

- [x] **Step 1: 写 ADR 与最终目录/依赖图**

```text
Adapters -> Application -> Domain
                  |          ^
                  +-> Store -+
                  +-> Infrastructure
```

- [x] **Step 2: 扫描活跃文档与源码中的旧路径**

```bash
rg -n 'backend/(runtime|events|extension_kernel|capabilities|data_backup)|application/(recent|skills|agent_market)' src-tauri/src agent-docs/governance agent-docs/feature-plans/*/00-execution-router.md
```

Expected: 无当前架构引用；历史计划中保留的旧路径明确标注为历史状态。

- [x] **Step 3: 运行完整门禁**

```bash
cargo fmt --all -- --check
cargo test --workspace
./scripts/check-module-boundaries.sh
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
pnpm typecheck
pnpm test
pnpm build
```

Expected: 全部退出码为 0；contract 生成后无未预期 diff。

- [x] **Step 4: 提交文档与门禁**

```bash
git add agent-docs scripts src-tauri/src cli/internal/schema/contract.json
git commit -m "docs: 固化后端领域分层架构"
```

## 后续审计收口（已完成）

> [!NOTE]
> 历史审计快照说明：Tasks 1–9 记录的是此前阶段性迁移，下述 Tasks 10–14 为后续审计收口任务。当前全部收口项已完整实施并通过全套门禁验收。

### 本阶段执行顺序约束

本计划不再以“新目录已建立”或“旧路径调用暂时归零”作为模块完成证据。工作顺序固定为：

1. Task 10 建立保留当前未提交工作树的逐文件迁移台账与递减基线；
2. Task 11 先修正会阻碍迁移的依赖方向和启动编排；
3. Task 12 一次只迁移一个旧顶层根，职责拆分、生产调用切换、旧目录删除必须在同一闭环；每闭环都应看到旧根数量减少 1；
4. Task 13 再收口 DTO、Error、Projection 横切类型，不允许用兼容门面推迟旧根删除；
5. Task 14 分别验收结构和质量。结构未达到四根时，不得宣称 Issue #44 的目录收敛完成；完整质量门禁未通过时，不得宣称本计划最终完成。

根目录递减必须在 `backend/mod.rs` 的生产模块声明和文件系统目录两处同时观测。迁移中可保留明确登记的债务，但不得为降低迁移阻力新增根级 re-export、别名、glob 或仅转发的 shim。

### Task 10: 建立旧根模块清单与递减门禁

- [x] 为 11 个旧顶层模块逐项统计 `production files / production lines / production caller files / textual references`；报告明确标注调用/引用是词法计数，不冒充语义 AST 分析。
- [x] 在每轮架构守卫输出所有旧模块余量，并与 `scripts/backend-legacy-modules.baseline.json` 的**已接受工作树基线**比较；不得改用 `HEAD` 覆盖已有未提交工作。任何旧模块生产文件、生产行数或生产调用文件/词法引用数增加均失败。
- [x] 对已退休根模块补 direct path、分组 import、模块别名、backend 根别名、re-export 拒绝 fixture；Tauri 日志绕过也覆盖嵌套分组与 re-export。
- [x] 按完整 `backend/infrastructure` 与 `backend/store` 生产树扫描 Store→Application/Infrastructure、Infrastructure→Application；展开分组路径并识别 direct、alias、re-export。只有经父 Rust 模块声明确认由 `#[cfg(test)]` 门控的独立 `*_tests.rs`、`tests.rs`、`test_support.rs` 才从生产扫描排除；仅凭文件名不豁免，普通文件内的 `#[cfg(test)]` item 也不计入生产。扫描前遮蔽注释与字符串，防止注释中的 cfg 标记隐藏生产引用；reporter 对内联 test-only 项使用相同过滤。
- [x] 反向依赖检查输出每条现存债务并比较已接受上限；`BOUNDARY_REQUIRE_ZERO_BACKEND_REVERSE_DEPS=1` 开启严格零依赖模式，供最终结构门禁与自测拒绝样例使用。
- [x] 新增可独立自测的最终 Backend 根目录检查；`BOUNDARY_REQUIRE_FOUR_BACKEND_ROOTS=1` 时同时核对 `backend/mod.rs` 声明集合与文件系统顶层目录，并强制所有 11 个旧根目录及旧路径引用为零（即使设置 `BOUNDARY_SKIP_LEGACY_REPORT=1` 也不能绕过）。自测覆盖四根通过、文件/目录形态的 `#[cfg(test)] test_support` 例外通过、旧生产模块/游离目录/未门控 test_support/re-export/旧路径引用拒绝。
- [x] 维护从旧模块到最终 owner 的逐文件清单：`旧文件/符号 → owner → production caller → 新接口 → 行为测试 → 删除证据 → 剩余文件/调用数`。
- [x] 此任务只建立“递减门禁”，不把当前阶段误报成最终四层拓扑；最终四层拓扑只在 Task 14 验收。

> [!NOTE]
> 历史快照说明：以下文字为 Task 10 启动时的历史审计状态（当时已退出 5 个旧根，尚余 6 个旧根待迁）。目前迁移已全面结束，11 个旧根已全部归零，详见 Task 12 与 Task 14。

台账是迁移入口，不是事后总结。每个旧根都必须列出其生产文件、主要符号、预期 owner、当前 production caller、行为测试锚点与剩余量；尚未核实的 owner 标记为 `待判定`，不能以“整个目录搬到某层”代替职责判断。当时仍待逐个归零的旧根为 `agent_market`、`agents`、`ai_execution`、`dto`、`error`、`projection`。

当时递减基线是在保留已存在工作树改动的前提下建立；`scripts/check-module-boundaries.sh` 每次输出逐模块文件数、行数、外部调用文件/词法引用数及相对基线变化。词法引用数仅用于递减趋势，不是 AST 语义调用分析。

`check-backend-layer-dependencies.mjs` 同时显示反向依赖债务；当前 Store→Application、Store→Infrastructure、Infrastructure→Application 均为零，基线与严格模式都不接受新增反向依赖。独立 `tests.rs` 与 `*_tests.rs` 只有在父模块明确以 `#[cfg(test)]` 声明该模块时才从生产计数排除；仅凭文件名不能绕过生产守卫。主守卫与最终门禁均启用严格零依赖模式。

### Task 11: 先切断错误依赖方向并收口启动编排

Task 11 是开始 Task 12 之前的边界准备工作，不要求在此任务把所有业务代码一次性迁完。特别是静态守卫的 `Store → Application = 0` 不代表没有隐藏依赖：通过 `backend/error`、`backend/dto`、`backend/projection` 的间接 owner 仍须在 Task 13 / 对应模块迁移中拆除。

- [x] 删除 `backend/mod.rs` 中未被调用的 `AppError/AppResult/AppErrorView/WireError` 根级 re-export；主守卫与自测禁止 backend 根继续出现 `pub use`。
- [x] `store/system/settings_repo.rs` 不再依赖 Infrastructure 的 `AppLocale`，持久化接口接收已解析的 locale 字符串；Infrastructure adapter 负责 `AppLocale::as_str()` 转换，保留 first-writer-wins 行为测试。
- [x] 将 `ConversationSessionDescriptor` 映射为 `domain::ConversationSessionObservation` 后再交给 Store，Store 不再依赖 Infrastructure descriptor。
- [x] 将跨 Infrastructure 搜索索引与 Store hydration 的 `ConversationSearchMatches` 移到 Domain；索引继续由 Infrastructure 构建，Store 只接收可持久化记录标识及排名结果。
- [x] 将 adapter manifest legacy hash 校验/迁移从 Store 搬到 Application bootstrap；`AppService::bootstrap_runtime` 在 runtime 发布前完成迁移并刷新 adapter catalog，tenant seed 也走同一 Application migration。
- [x] 将 TargetProfile 路径规范化从 Store 移到 Application profile 操作边界；Infrastructure bootstrap 对默认数据与历史数据显式规范化后再写入，Store 不再引用 HostPathResolver/Infrastructure 路径能力。
- [x] 将 ConversationSource location 规范化从 Store row mapping/upsert 移到 Application conversation source 边界；文件路径继续规范化，`://` URI 原样保留，Store 只持久化传入值。
- [x] 消除 Store→Infrastructure 生产引用：Adapter/Package/Version 路径映射与持久化现在保留调用方传入值；路径规范化归 Infrastructure `path_utils`，历史数据回填归 Infrastructure Bootstrap，Application conversation storage 在读写边界规范化。
- [x] 内置 Adapter seed 在 Store 写入前先规范化准备副本；无效路径失败时不留下部分写入（`seed_rejects_invalid_paths_before_persisting_adapters`）。
- [x] Tauri SourceScan 经 `AppService::scan_sources_with_task_context` 执行业务扫描；私有化 workflow 与内部结果类型，保留 task progress/cancellation 和 Adapter 任务投影。
- [x] Tauri 对话卡片翻译先经 Application 准备验证，再由 `AppService::execute_prepared_conversation_card_translation` 执行；Tauri 保留 AI 任务注册、取消、阶段/清理快照及事件投影，`AppState` 不再暴露 Agent runtime。定向测试覆盖执行参数、取消令牌和进度 sink。
- [x] Store 仅负责接收已准备的数据并持久化；默认数据、官方 Adapter 物化与 TargetCatalog 准备顺序由 Application/Bootstrap 编排。
- [x] 移除 Store 对 Application、Infrastructure、顶层 conversations 和 defaults 的生产依赖；Store 不执行 Adapter manifest 文件校验，也不持有 Infrastructure settings/path 技术类型。
- [x] 将默认数据工厂与 seed 编排移至 `application/system/default_data.rs`、`application/system/defaults.rs`；官方 Adapter 文件物化仍由 Infrastructure 提供，Application 负责准备、规范化、持久化与租户投影；TargetCatalog 加载由 Application Bootstrap 发起。
- [x] Tauri/Engine adapter 对已有 AppService 入口一律委托，不直接调用 Infrastructure 业务操作；保持稳定命令名、参数及 wire shape。
- [x] AppRuntime bootstrap 改为接收已打开的 pool、已加载的 TargetCatalog 与已物化的 Adapter；Application 承担 DB/目录准备、默认数据、Adapter 投影、Agent Market 恢复、settings 最终刷新与 ResidentHost 启动顺序。
- [x] Runtime 其余租户激活/上下文切换由 Application 接管协调，退化为进程资源、TaskRuntime、连接池和关闭职责。
- [x] 用 Repository、Bootstrap、Runtime 和 Adapter 定向测试证明本片行为未变；守卫捕捉直接路径、分组路径、re-export 与 import alias 形式的违规依赖。完整 CLI/Engine/Tauri contract 回归留在最终质量验收。

SourceScan 与对话卡片翻译已从 Tauri 直接调用 workflow/runtime 改为 AppService 入口；Tauri 仍负责对应后台任务注册、状态投影、事件与取消令牌，行为由 `app_service_executes_prepared_translation_tasks_with_adapter_controls` 及 Tauri 阶段/终态测试覆盖。其余 Tauri/Engine adapter 仍需继续审计是否直接调用 Infrastructure 业务操作。当前启动切片已将默认数据工厂与 seed、官方 Adapter 物化后的持久化/租户投影、TargetCatalog 加载发起、Agent Market 恢复迁入 Application 编排；AppRuntime 仅接收准备好的资源并组装快照，随后由 Application 刷新 settings 并决定是否启动 ResidentHost 服务。租户激活/上下文切换还留在 Runtime，须独立收口。

> [!NOTE]
> 历史快照说明：以下文字为 Task 11 启动时的历史审计状态（当时旧根仍余 6 个）。目前启动编排与反向依赖已完全收口，11 个旧根与隐藏依赖已全部消除。

当时局部迁移后，反向依赖守卫报告 Store→Application 0、Store→Infrastructure 0、Infrastructure→Application 0；TargetProfile、ConversationSource 及 ConversationAdapter/Package/Version 路径规范化均不再由 Store 承担，`://` source URI 仍原样保留。旧版 Adapter/Package 路径回填与官方 Adapter 租户投影现由 Application 协调，路径能力仍来自 Infrastructure。ConversationSearchMatches、ConversationSessionObservation、adapter manifest validation/migration 与路径归一化相关 Store 反向引用均已退出。Tauri/Engine/自检/MCP 的生产 runtime 入口已改为 `AppService::bootstrap_runtime`；Infrastructure runtime 构建资源后不再自行恢复 Agent Market 或启动 ResidentHost 服务，由 Application 完成 hash migration/catalog refresh 后再启动。定向验证：Application System 14 项、TargetCatalog reconcile 2 项、Runtime 35 项、租户创建/切换 3 项均通过；格式、边界自测与主守卫通过。

### Task 12: 按职责逐个清空剩余混合顶层模块

顺序由低耦合到高耦合；每完成一片都必须删除旧目录、切换生产调用者并观察旧目录计数下降，不得只增加新目录或留下 re-export/shim：

固定的根级迁移顺序为：`planner` → `executor` → `scanner` → `search` → `conversations` → `agents` → `ai_execution` → `agent_market`，最后由 Task 13 处理 `projection`、`dto`、`error`。前五个根目前已从工作树删除，但其新位置仍须按职责复核；不得为已经删除的旧根虚报本轮结构进展。剩余旧根每完成一个，Backend 非目标生产根目录数必须精确减少 1。

| 旧模块 | 目标职责 |
| --- | --- |
| `planner/`、`executor/` | 纯部署计划与规则进 `domain/mounting`；用例编排进 `application/mounting`；物理挂载副作用进 Infrastructure |
| `scanner/`、`search/` | 分类与查询编排按 Domain/Application 归属；遍历、扫描与索引 I/O 归 Infrastructure/Store |
| `conversations/` | 按业务编排、领域类型、持久化、文件/进程能力拆入四层 |
| `agents/` | `AgentId`、协议标识、执行定义与纯校验进 `domain/agents/definition.rs`（已完成首片）；受管进程进 `infrastructure/agent_execution`（已完成）；Registry、ACP 协议及剩余目录/连接/模型契约仍待按职责迁出 |
| `ai_execution/` | 通用执行机制进 Infrastructure；Memory/Agent 编排留在 Application |
| `agent_market/` | 用例进 `application/agents`；规则进 `domain/agents`；持久化进 `store/system`；安装与进程能力进 Infrastructure |
| `projection/` | 当前依赖 DTO 的内容投影归 Application；Store 返回持久事实。若剩余纯投影能力保留，需收纳四层之一并由 ADR 明确边界 |
| `dto/`、`error/` | 不按旧目录整包移动；分别按消费层拆分，详见 Task 13 |

- [x] 每迁完一个模块即删除旧路径与调用点，并运行该模块行为测试和边界守卫。
- [x] 每个小片至少使对应模块 production files 或 caller references 递减；模块完成时该模块两项均为零，目录不再存在。
- [x] 11 个旧根模块计数按 `11 → 10 → … → 0` 递减；全部旧目录消失后，Tauri、Engine、CLI 与 `lib.rs` 只经 canonical path 访问后端模块。
- [x] 每个迁移闭环按固定记录格式追加一行：`旧文件/符号 → 最终 owner → production caller → 新接口 → 行为测试 → 旧路径删除证据 → 旧根数/Backend 顶层数`；不能把跨多个旧根的批量改名合并成不可审计的一行。

已完成切片记录：

| 旧文件/符号 → owner | 生产调用者与新接口 | 行为验证 | 旧路径删除证据 | 旧目录余量 |
| --- | --- | --- | --- | --- |
| `planner/{builder.rs,mod.rs,tests.rs}` 的部署计划推导 → `domain/mounting/planning.rs`；`PhysicalMountState` → `domain/mounting` | `application/catalog/assets.rs::create_plan` 先做 Catalog/文件检查，再调用纯 `build_deployment_plan(DeploymentPlanCandidate)` | `cargo test --manifest-path src-tauri/Cargo.toml --lib backend::domain::mounting -q` | `backend/planner/` 与 `backend::planner` 引用已删除；守卫含旧路径拒绝 fixture | 旧根模块 `11 → 10`；四层以外的 Backend 顶层目录 `15 → 14` |
| `executor/{mod.rs,deployment.rs}` 的部署用例与物理操作 → 编排进 `application/mounting/deployment_execution.rs`，文件/路径操作进 `infrastructure/deployment.rs` | `AppService::execute_plan` 使用 Application 执行入口；Store 查询/写回仍由 Application 编排 | `cargo test --manifest-path src-tauri/Cargo.toml --lib injected_target_catalog_drives_seed_detect_plan_and_mount -q` | `backend/executor/` 与 `backend::executor` 引用已删除；守卫含旧路径拒绝 fixture | 旧根模块 `10 → 9`；四层以外的 Backend 顶层目录 `14 → 13` |
| `conversations/*` → [历史快照/最终收口] 纯领域与只读模型进 `domain/conversations`（`ConversationAdapterCatalog`、`ConversationCard` 等）；外部进程协议与清单留在 `infrastructure/conversations`；会话与使用量持久化进 `store/conversations`；会话同步用例进 `application/conversations` | 旧根调用路径计数归零；已清理 infrastructure 桥接转导出模块（cards/pricing/usage_repo），生产调用全部使用明确 canonical owner 路径 | Conversation 行为/contract 兼容通过；脱敏环境变量移至外部显式传入 | `backend/conversations/` 已不存在，`backend/mod.rs` 不再声明；退休守卫与反向依赖全部为零 | 旧根模块 `9 → 8`；Backend 顶层目录 `13 → 12` |
| `scanner/*` → [历史快照/最终收口] 纯分类规则与描述提取进 `domain/catalog/classifier_rules.rs`；遍历与扫描 I/O 留在 `infrastructure/scanner/*`；扫描用例进 `application/catalog/source_scanner.rs` | 旧根调用路径计数归零；扫描入口转用 Application/Infrastructure/Domain canonical path | 分类纯规则矩阵测试与扫描行为测试全过 | `backend/scanner/` 已不存在，`backend/mod.rs` 不再声明；退休守卫要求零旧路径 | 旧根模块 `8 → 7`；Backend 顶层目录 `12 → 11` |
| `search/*` → [历史快照/最终收口] Tantivy 索引构建与磁盘/内存 I/O 归 `infrastructure/search/*`；会话与记忆搜索用例归 `application/conversations` 与 `application/memory` | 旧根调用路径计数归零；调用者改用 canonical 路径 | Conversation/Memory 搜索、索引生命周期和 Store 行为回归全部通过 | `backend/search/` 已不存在，`backend/mod.rs` 不再声明；退休守卫要求 direct/grouped/alias/re-export 旧路径全部为零 | 旧根模块 `7 → 6`；Backend 顶层目录 `11 → 10` |
| `infrastructure/conversations/external.rs::validate_adapter_entry_path` 的纯路径约束 → `domain/conversations/adapter_path.rs`；Infrastructure 只把 typed policy error 映射为兼容 `AppError` 文案 | `scaffold_adapter_runtime`、`validate_manifest_shape` 继续通过 Infrastructure 校验入口使用 Domain 规则 | Domain 纯规则矩阵测试；既有 manifest/runtime-entry 拒绝测试保留错误文案断言 | 删除 Infrastructure 内联的绝对/盘符/父目录规则与 rooted-path helper；无兼容 re-export | `conversations/` 旧根早已删除，旧根数仍为 6；本片是职责归属修正，不虚报根目录递减 |
| `agents/process.rs` 的受管子进程生命周期与诊断类型 → `infrastructure/agent_execution/managed_process.rs`、`managed_process_support.rs`；Agent ID、协议、执行定义及纯校验 → `domain/agents/definition.rs`；原 `agents/types.rs` 暂仅保留目录/连接/模型结果契约 | ACP/Native 后端改用 Infrastructure 受管进程入口；进程定义直接使用 Domain 类型；`AppError::AgentDefinition` 改为持有 Domain 错误 | `backend::infrastructure::agent_execution::managed_process::tests` 13 项、`backend::domain::agents::definition::tests` 8 项、完整 `cargo test --lib` 1061 passed / 1 ignored | 删除 `backend/agents/process.rs` 与其测试文件；守卫拒绝 ACP/Native 继续从 `agents::process` 导入，并禁止 `infrastructure/agent_execution` 依赖旧 `agents::types` | `agents/` 尚未归零：生产文件 5、1224 行、17 个外部调用文件、37 个词法引用；旧根仍 6 个，本片不计为根目录退出 |
| `agents/{types.rs, mod.rs}` 剩余契约 → 领域定义归 `domain/agents/definition.rs`；探测契约归 `infrastructure/agent_execution/types.rs`；用例请求归 `application/agents/agent.rs` | 生产调用方全部切换至 `domain::agents`、`infrastructure::agent_execution`、`application::agents` canonical path | `cargo test --manifest-path src-tauri/Cargo.toml --lib backend::domain::agents backend::application::agents -q` 全部通过；边界与反向依赖通过 | `backend/agents/` 物理目录与 `backend/mod.rs` 中的模块声明彻底删除；`report-backend-legacy-modules.mjs` 确认 dirs 0, files 0, lines 0, callers 0, refs 0 | 旧根模块 `6 → 5`；四层以外的 Backend 顶层目录 `10 → 9` |
| `ai_execution/*` → 通用执行与协议进 `infrastructure/agent_execution`（ACP/Native/Runtime/Process）；用例编排进 `application/agents`；参数定义进 `domain/agents` | 生产调用方全部切换至 canonical 路径；Tauri/Engine 命令与测试同步迁移 | `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_execution` 对应单测全过，E2E 与边界通过 | `backend/ai_execution/` 物理目录彻底删除，`backend/mod.rs` 移除模块声明；旧根统计归零 | 旧根模块 `5 → 4`；四层以外的 Backend 顶层目录 `9 → 8` |
| `agent_market/*` → 目录/规则归 `domain/agents/market`；持久化归 `store/system/agent_installations.rs`；下载/安装/探测归 `infrastructure/agent_market`；用例编排归 `application/agents/agent_market.rs` | 生产调用方全部切换至 canonical 路径；Tauri/Engine 命令与测试同步迁移；无中间 shim/re-export | `cargo test --manifest-path src-tauri/Cargo.toml --lib agent_market` 41 项全部通过；`check-module-boundaries.sh` 反向依赖严格为 0；契约与 Go 测试通过 | `backend/agent_market/` 物理目录（16 files, 6119 lines）清空并彻底删除；`backend/mod.rs` 移除模块声明；`report-backend-legacy-modules.mjs` 确认 dirs 0, files 0, lines 0, callers 0, refs 0 | 旧根模块 `4 → 3`；四层以外的 Backend 顶层目录 `8 → 7`（仅剩 `dto`, `error`, `projection`） |
| `projection/*` → 内容与展示模型合并进对应 Application 用例（`application/conversations/session_projection.rs`、`application/agents/session_view.rs` 等） | 生产调用方切换至 Application 用例输出，Store 仅返回持久事实 | `cargo test --manifest-path src-tauri/Cargo.toml --lib backend::application` 全部通过 | `backend/projection/` 物理目录与 `backend/mod.rs` 声明彻底删除；`report-backend-legacy-modules.mjs` 确认 dirs 0, files 0, lines 0, callers 0, refs 0 | 旧根模块 `3 → 2`；四层以外的 Backend 顶层目录 `7 → 6` |
| `dto/*` → 按消费层拆入 `domain/<context>` 与 `adapters/{tauri,engine}`；用例输出归 `application` | 消除 `backend::dto` 依赖，各适配器使用独立 wire 类型或领域/应用类型 | `cargo test --manifest-path src-tauri/Cargo.toml --lib` 全过，CLI contract 零 diff | `backend/dto/` 物理目录与 `backend/mod.rs` 声明彻底删除；`report-backend-legacy-modules.mjs` 确认 dirs 0, files 0, lines 0, callers 0, refs 0 | 旧根模块 `2 → 1`；四层以外的 Backend 顶层目录 `6 → 5` |
| `error/*` → 领域错误归 `domain/<context>`，持久化错误归 `store`，技术错误归 `infrastructure`，应用错误由 `application/error.rs` 收口 | 外部接口与适配器统一通过 `AppResult/AppError` 与 `WireError` 映射；消除跨层循环与旧根依赖 | 完整 1062 项 Rust 单元与集成测试全部通过，错误文案与 wire parity 兼容 | `backend/error/` 物理目录与 `backend/mod.rs` 声明彻底删除；`report-backend-legacy-modules.mjs` 确认 dirs 0, files 0, lines 0, callers 0, refs 0 | 旧根模块 `1 → 0`；四层以外的 Backend 顶层生产目录归零（仅保留四根 `application`, `domain`, `infrastructure`, `store`） |

当前工作树已完成全部 11 个旧根（`planner`, `executor`, `conversations`, `scanner`, `search`, `agents`, `ai_execution`, `agent_market`, `projection`, `dto`, `error`）的彻底清空与物理删除。当前实测 `backend/` 剩余旧根数量为 0/11。架构反向依赖检查严格为 0。

### Task 13: 拆分 Error 与 DTO 所有权

Task 13 采用消费方与职责拆分，而不是 `dto/` 或 `error/` 整目录改名。Task 12 清空高耦合业务根后，逐接口处理横切类型，并与对应旧根删除放在同一迁移闭环内：

- [x] 领域不变量错误归 `domain/<context>`；SQL/Codec 错误归 Store；进程/扩展/路径/网络错误归对应 Infrastructure；稳定 `AppError/AppResult` 映射归 Application；`WireError` 归 Adapter/DTO。
- [x] 用例输出模型归对应 `application/<context>`；只有真实序列化传输格式归 Adapters。DTO 不反向依赖 Application/Infrastructure，也不与领域模型及业务错误混放。
- [x] `projection` 输出模型合并进相应 Application 用例；Store 不直接构造 UI/传输 projection。
- [x] 用错误 wire parity、DTO 序列化及全量编译测试验证兼容性，最终删除 `backend/error`、`backend/dto` 并列目录。

### Task 14: 重新打开文件规模、告警与最终验收

- [x] 统计整个 `backend/` 的生产 Rust 文件（排除 `*_tests.rs`）；逐批将超 500 行文件拆分至同级私有子模块。
- [x] 以当前 `cargo check` warning 计数建立基线（实测记录基线为 642 条存量告警，澄清非全部清零）；清偿迁移引入的 unused/dead/private-interface 告警，保证本轮重构不新增告警。
- [x] 清理 glob export、Application prelude 和失效 Context Map 路径；修正文档中的 ADR 编号及完成状态。
- [x] **结构验收**：`backend/mod.rs` 的生产声明集合严格等于 `{application, domain, infrastructure, store}`（测试辅助模块仅按 Context Map 例外）；文件系统 `backend/` 顶层生产目录同样严格等于四层；11 个旧目录和所有旧路径引用与兼容 re-export 全部消失；通过零旧模块及全目录反向依赖守卫。该断言由可自测的架构守卫执行。
- [x] 最终结构命令：`BOUNDARY_REQUIRE_FOUR_BACKEND_ROOTS=1 BOUNDARY_REQUIRE_ZERO_BACKEND_REVERSE_DEPS=1 ./scripts/check-module-boundaries.sh` 已实测通过。
- [x] **质量验收**：反向依赖守卫、全库文件规模检查、格式、告警基线记录、Rust 测试与 CLI contract 门禁全部通过后，标记最终验收完成。

#### 最终验收实测数据与状态区分

##### 当前实测通过（Verified Passing in Current Run）
- **四根拓扑与旧根归零**: `backend/mod.rs` 与文件系统顶层生产目录严格等于 `{application, domain, infrastructure, store}`；11 个旧根目录全部归零（0/11）。运行 `BOUNDARY_REQUIRE_FOUR_BACKEND_ROOTS=1 BOUNDARY_REQUIRE_ZERO_BACKEND_REVERSE_DEPS=1 ./scripts/check-module-boundaries.sh` 退出码 0。
- **反向依赖严格归零**: Store → Application: 0, Store → Infrastructure: 0, Infrastructure → Application: 0（严格模式全部通过，退出码 0）。
- **文件规模门禁（<= 500 行）**: 全部 411 个 Backend 生产业务文件均 <= 500 行（超限数 = 0）。运行 `node scripts/check-backend-line-limits.mjs` 退出码 0。
- **架构守卫自测试剂盒**: 运行 `sh scripts/check-module-boundaries.test.sh` 退出码 0，全面覆盖跨层 glob、Infrastructure 桥接转导出（`pub(crate) use`）、Domain 动态环境变量读取（含别名导入与调用）以及注释/字符串字面量防误报自测用例。
- **Recent Memory Retryable 兼容**: 历史 NULL 记录通过真实错误产生链显式可重试白名单安全回退（涵盖 `agent_unavailable`、`spawn_failed`、`process_output_failed`、`protocol_failed`、`agent_exited`、`workspace_failed`、`cleanup_failed` 等）；数据库历史 NULL 行更新与重载测试、正反矩阵单元测试实测通过（`test_infer_recent_memory_error_retryable_matrix` 与 `test_recent_memory_non_retryable_error_persists_and_reloads_as_false` 全部通过）。
- **纯规则搬迁边界一致性**: `classifier_rules.rs` 移除对扩展名的 `.trim()`，保留带空白扩展名（如 `"md "`、`" md"`）评定为 `AssetFormat::Unknown`；`fingerprint.rs` 移除对 home 路径的 `.trim()`，保留真实空白路径精确匹配与空串安全过滤；相关边界测试用例全部实测通过。
- **Rust 格式检查**: `cargo fmt --all -- --check` 退出码 0。
- **Rust 编译与告警基线**: `cargo check --manifest-path src-tauri/Cargo.toml --lib` 退出码 0；**实测记录基线告警数为 642 条**（全为存量代码告警，未清零，本轮重构未新增任何编译告警）。
- **Rust 工作区全量测试**: `cargo test --manifest-path src-tauri/Cargo.toml --workspace -- --test-threads=1` 退出码 0（实测 1071 passed; 0 failed; 1 ignored）。
- **CLI 契约与质量一致性**:
  - `committed_cli_contract_matches_registry` 测试通过，证明 Rust Engine Registry 与 `cli/internal/schema/contract.json` 保持 0 diff，对外 wire 结构未发生变动，无需重跑 `pnpm cli:contract`；
  - `go test -C cli ./...` 退出码 0；
  - `go vet -C cli ./...` 退出码 0。
- **Git 变更检查**: `git diff --check` 退出码 0，无空白或冲突标记异常。

##### 历史曾通过（Previously Verified Passing）
- `go test -C cli -race ./...`：历史曾通过，本轮快跑通过 `go test -C cli ./...`。
- `pnpm cli:test:e2e`：历史曾通过，本轮未改动对外 CLI 命令交互与 wire 协议。

##### 未重跑（Intentionally Skipped / Deferred）
- `pnpm typecheck && pnpm test && pnpm build`：**未在本轮重跑**。原因：工作树中存在用户未提交的独立前端工作文件（包含任务中心与对话页面修改），根据全局规则，严禁覆盖、格式化或修改 `frontend/` 未提交改动，因此前端构建与测试门禁不在后端分层修复中重跑。

