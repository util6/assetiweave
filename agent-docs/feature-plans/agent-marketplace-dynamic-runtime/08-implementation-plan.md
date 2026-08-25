# 实施计划：Agent Marketplace 与动态运行时 (Implementation Plan)

| 字段 | 值 |
|---|---|
| 状态 | Pending，等待 SPEC 人工评审 |
| 任务原则 | 单任务 1–5 个手写文件；测试先行；逐 checkpoint；禁止跨任务扩展 |
| 任务总数 | T00–T33，含 T15A（共 35 项） |

## 1. 执行规则

1. 一次只执行一个 Task ID。
2. 开始前读取仓库根 `AGENTS.md`、主索引、本任务指定分册和代码文件。
3. 先写失败测试，再实现最小代码。
4. 手写文件超过 5 个时停止并拆任务；生成 contract/artifact 可作为明确例外，但必须列出。
5. 不修改任务白名单外文件；发现必要依赖时先更新本计划并评审。
6. 不复制 Conversation package manager 的大文件、hash/trust/edit/history 设计。
7. 所有网络、process、filesystem、clock、Registry publish 通过可注入 seam 测试。
8. 每项完成后更新 `10-progress.md`，记录命令、PASS/FAIL 和偏差。
9. commit 使用中文 Conventional Commit，且一个 commit 只对应一个 Task 或一个不可分 checkpoint。
10. 用户已有未提交改动不属于任务时不得改写或回退。

## 2. 全局 Stop Conditions

以下任一发生时停止当前任务，不自行扩大范围：

- 本 SPEC 与实际代码/上游 Schema 有实质冲突。
- 需要允许远程 catalog 提供任意 command/env/hook。
- 需要执行期联网、`npx -y` 或临时 `uvx` 才能通过测试。
- 需要新增自定义包、Git/local source、自动更新或版本历史。
- 需要修改超过 5 个手写文件。
- 只能依赖真实 Vendor/npm/PyPI 网络才能测试。
- 需要删除或覆盖不属于当前 Task 的未提交修改。
- 无法保证 update 失败保留旧安装，或无法保证 System 卸载不删外部文件。
- 需要把 ACP 失败映射成 CLI execution fallback。

停止报告必须给出：冲突事实、受影响 Requirement、2–3 个方案、推荐方案、需人工决定的问题。

## 3. 依赖图

```text
T00 -> T01 -> T02 -> T03
             ├-> T04 -> T05 -> T06
             └-> T12

T06 -> T07 -> T08 -> T09 -> T10
T07..T10 -> T11
T03 + T11 + T12 + T15A -> T16 -> T17 -> T18 -> T19

T12 -> T13 -> T14 -> T15 -> T15A
T13 -> T26

T16..T19 + T14..T15A -> T20 -> T21 -> T22 -> T23 -> T24
T20 -> T25 -> T26
T20..T22 -> T27 -> T28 -> T29 -> T30 -> T31
T24 + T26 + T31 -> T32 -> T33
```

## 4. Phase 0：决策冻结与基线

### T00 人工评审并冻结 SPEC

**状态**：待开始
**目标**：确认主索引 D-101..D-116、默认资源预算和非目标。
**涉及文件**：仅本 SPEC 文档集。
**实施步骤**：

1. 评审产品名称 Agent Market。
2. 确认 curated index、single active installation、manual update。
3. 确认 System/Binary/Npx/Uvx 和 app-managed Runtime 非目标。
4. 确认 OpenCode 无 CLI execution fallback。
5. 将文档状态从 Proposed 改为 Approved，并记录评审人/日期。

**验收标准**：所有冻结项有明确结论；偏离项已同步所有分册。
**验证命令**：文档评审。
**依赖任务**：无。

### T01 固化当前行为 Characterization Tests

**状态**：待开始
**目标**：用测试记录当前九 Agent、`cli_fallback` 误语义、Registry/OnceLock 边界，后续变更有对照。
**先读**：主索引 §4；`05` §1。
**涉及文件**：

- `src-tauri/src/backend/agents/registry.rs`
- `src-tauri/src/backend/agents/types.rs`
- `src-tauri/src/backend/ai_execution/executor.rs`
- `src-tauri/src/backend/application/system.rs`

**优先编写测试**：记录九条 builtin definition；OpenCode ACP fail 当前 compatibility 分支；不同 DB path singleton 现状。
**验收标准**：只加 characterization，不改变生产行为；测试名明确标注将被 T12/T14/T26 替换。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution::executor
cargo test --manifest-path src-tauri/Cargo.toml backend::application::system
```

**依赖任务**：T00。

## 5. Phase 1：领域类型、Schema 与 Store

### T02 建立 Agent Market 领域类型

**状态**：待开始
**目标**：定义 catalog、distribution、installation、health、task、error 类型，不实现 I/O。
**先读**：`01` §4–6；`03` §3–5；`04` §1–2/14。
**涉及文件**：

- `src-tauri/src/backend/agent_market/mod.rs`（new）
- `src-tauri/src/backend/agent_market/types.rs`（new）
- `src-tauri/src/backend/mod.rs`

**优先编写测试**：CAT-05..08、readiness truth table、error serialization/redaction。
**验收标准**：

- Protocol 与 Distribution 正交。
- System/Binary/Npx/Uvx 为 tagged union。
- installed/connected/execution_ready 独立。
- 无 user command/env、hash trust、version history 类型。

**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::types
```

**依赖任务**：T01。

### T03 新增 Migration 与 Installation Repository

**状态**：待开始
**目标**：实现 `agent_installations` 单表和 tenant-scoped repository。
**先读**：`06` §1；`07` §4.2。
**涉及文件**：

- `src-tauri/migrations/NEXT_agent_installations.sql`（new，替换 NEXT）
- `src-tauri/src/backend/agent_market/repository.rs`（new）
- `src-tauri/src/backend/agent_market/mod.rs`
- `src-tauri/src/backend/store/mod.rs`

**优先编写测试**：tenant isolation、CHECK constraints、upsert/list/candidate/health/delete、transaction rollback。
**验收标准**：DDL 与 `06` 一致；一个 tenant/agent 一条 current row；无 catalog/history/hash 表。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::repository
cargo test --manifest-path src-tauri/Cargo.toml backend::store
```

**依赖任务**：T02。

### Checkpoint A：Domain/Store

```bash
cargo fmt --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market
cargo test --manifest-path src-tauri/Cargo.toml backend::store
```

通过后人工检查：表数量、状态枚举、ownership path invariant、无 Conversation hash/trust 类型。

## 6. Phase 2：Catalog 与选择

### T04 实现 Catalog Parser 与 Bundled Catalog

**状态**：待开始
**目标**：解析/校验 v1 schema，并提供首批九 Agent 的 bundled 精选目录。
**先读**：`03` §2–6/11/12。
**涉及文件**：

- `src-tauri/src/backend/agent_market/catalog.rs`（new）
- `src-tauri/src/backend/agent_market/types.rs`
- `src-tauri/src/backend/agent_market/mod.rs`
- `builtin-assets/agent-market/catalog-v1.json`（new）
- `src-tauri/src/backend/mod.rs`（仅资源接入确需时）

**优先编写测试**：CAT-01、CAT-05..09；九 Agent 分发形态断言；Kiro/Qoder command 漂移回归。
**验收标准**：固定版本、无 latest/range、唯一 ID、标准 ACP item 数据驱动；所有来源字段可追踪。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::catalog
```

**依赖任务**：T02。

### T05 实现 Catalog 原子缓存与 ETag 刷新

**状态**：待开始
**目标**：best-valid catalog 读取、受控 fetch、ETag、原子 cache 和 offline fallback。
**先读**：`03` §2；`07` §2.1/3。
**涉及文件**：

- `src-tauri/src/backend/agent_market/catalog.rs`
- `src-tauri/src/backend/agent_market/cache.rs`（new）
- `src-tauri/src/backend/agent_market/mod.rs`
- `src-tauri/src/backend/app_settings.rs`（仅统一 cache path）

**优先编写测试**：CAT-02..04、5 MiB limit、invalid cache、atomic replace failure、ETag 304。
**验收标准**：fetcher/clock/filesystem 可注入；失败不覆盖旧 catalog；无用户自定义 URL。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::cache
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::catalog
```

**依赖任务**：T04。

### T06 实现 Distribution Selection 与 Preview Plan

**状态**：待开始
**目标**：按平台/Runtime/System version 生成确定候选和 preview plan。
**先读**：`03` §7–10；`04` §3；`06` §2.2–2.3。
**涉及文件**：

- `src-tauri/src/backend/agent_market/distribution.rs`（new）
- `src-tauri/src/backend/agent_market/types.rs`
- `src-tauri/src/backend/agent_market/mod.rs`

**优先编写测试**：DST-01..03、07、10；显式不可用 choice 不 fallback；preview token 稳定/变更。
**验收标准**：默认 System > Binary > Npx > Uvx；选择算法无网络/文件写；request 不含 program/args/env。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::distribution
```

**依赖任务**：T05。

## 7. Phase 3：Distribution Installers

### T07 建立 Installer Trait、Staging 与 System Installer

**状态**：待开始
**目标**：建立统一 `MaterializedRuntime`/installer seam，并实现只绑定不拥有的 System。
**先读**：`03` §5.1/8；`04` §5；`07` §2.5。
**涉及文件**：

- `src-tauri/src/backend/agent_market/installers/mod.rs`（new）
- `src-tauri/src/backend/agent_market/installers/system.rs`（new）
- `src-tauri/src/backend/agent_market/mod.rs`
- `src-tauri/src/backend/agent_market/types.rs`

**优先编写测试**：compatible/incompatible/missing version probe；argv no-shell；System uninstall ownership metadata。
**验收标准**：installer 不写 DB/Registry；System `install_dir=None`；不 hash/copy/chmod/delete executable。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::installers::system
```

**依赖任务**：T06。

### T08 实现 Binary Installer 与安全解压

**状态**：待开始
**目标**：受限下载、SHA-256、archive 安全解压和本地 program 解析。
**先读**：`03` §5.2/8–9；`07` §2.2/3。
**涉及文件**：

- `src-tauri/src/backend/agent_market/installers/binary.rs`（new）
- `src-tauri/src/backend/agent_market/installers/mod.rs`
- `src-tauri/src/backend/agent_market/types.rs`
- `src-tauri/Cargo.toml`（仅确需 archive/hash 依赖）
- `Cargo.lock`（generated）

**优先编写测试**：DST-04..06；wrong target；cancel；普通 fixture success；redirect/size limits。
**验收标准**：解压前 hash；拒绝 traversal/links/special files/duplicates；全部输出位于 staging；无真实网络测试。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::installers::binary
```

**依赖任务**：T07。

### T09 实现 Npx Materializing Installer

**状态**：待开始
**目标**：用 host npm 将固定包安装到 staging，生成 lock/integrity 并解析 local bin。
**先读**：`03` §5.3；`07` §2.3。
**涉及文件**：

- `src-tauri/src/backend/agent_market/installers/npx.rs`（new）
- `src-tauri/src/backend/agent_market/installers/mod.rs`
- `src-tauri/src/backend/agent_market/types.rs`

**优先编写测试**：DST-07..09；fake npm argv；scripts denied；lock mismatch；local bin boundary；cancel/timeout。
**验收标准**：exact package/version；强制 ignore scripts；结果 program local；执行 definition 无 `npx/-y`。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::installers::npx
```

**依赖任务**：T07。

### T10 实现 Uvx Persistent Tool Installer

**状态**：待开始
**目标**：用 host uv 在 staging 物化固定 Python tool，解析 app-owned command。
**先读**：`03` §5.4；`07` §2.4。
**涉及文件**：

- `src-tauri/src/backend/agent_market/installers/uvx.rs`（new）
- `src-tauri/src/backend/agent_market/installers/mod.rs`
- `src-tauri/src/backend/agent_market/types.rs`

**优先编写测试**：DST-10..11；fake uv env/argv；exact spec；global tool dir 未触碰；cancel/timeout。
**验收标准**：`UV_TOOL_DIR/BIN_DIR` 指向 staging；结果 program local；执行 definition 无 `uvx`。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::installers::uvx
```

**依赖任务**：T07。

### Checkpoint B：Distribution

```bash
cargo fmt --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::catalog
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::distribution
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::installers
```

人工检查：四种 fixture、无 shell、无执行期 package manager、System 不拥有文件、资源常量集中。

## 8. Phase 4：Conformance、Registry 与 Runtime Ownership

### T11 实现安装后 Conformance Service

**状态**：待开始
**目标**：复用现有 ACP/Native process/backend 能力验证 materialized runtime，不发送 prompt。
**先读**：`04` §10；前置 ACP SPEC `04-acp-process-runtime-design.md`。
**涉及文件**：

- `src-tauri/src/backend/agent_market/conformance.rs`（new）
- `src-tauri/src/backend/agent_market/mod.rs`
- `src-tauri/src/backend/agents/protocol/acp.rs`（仅增加最小可复用 probe seam）
- `src-tauri/src/backend/agents/process.rs`（仅确需 test seam）

**优先编写测试**：initialize/session/new/close；无 prompt/MCP；permission denied；timeout/cancel/child cleanup；model optional。
**验收标准**：不复制 ACP client；输出 typed health；所有 terminal path 无 process tree。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::conformance
cargo test --manifest-path src-tauri/Cargo.toml backend::agents
```

**依赖任务**：T08、T09、T10。

### T12 将 AgentRegistry 改为不可变动态快照

**状态**：待开始
**目标**：删除 hardcoded Registry source，加入 generation/snapshot/atomic reload handle。
**先读**：`02` §6；`04` §12。
**涉及文件**：

- `src-tauri/src/backend/agents/registry.rs`
- `src-tauri/src/backend/agents/types.rs`
- `src-tauri/src/backend/agents/mod.rs`

**优先编写测试**：empty fresh Registry；one definition；duplicate/invalid definition fail；old snapshot survives failed publish；concurrent read sees old/new complete snapshot。
**验收标准**：无 builtin definitions；Registry 不依赖 DB/catalog；lookup 纯内存；只接受 Runtime Manager 已筛选的 execution-ready definitions；generation 成功时递增。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::registry
```

**依赖任务**：T02、T03。

### T13 改造 AgentExecutor 的 Registry Handle 与 Active Identity

**状态**：待开始
**目标**：execution clone definition snapshot，并记录 agent/installation identity 供生命周期冲突检查。
**先读**：`02` §6/8；`04` §12。
**涉及文件**：

- `src-tauri/src/backend/ai_execution/executor.rs`
- `src-tauri/src/backend/ai_execution/types.rs`
- `src-tauri/src/backend/agents/registry.rs`

**优先编写测试**：active count per Agent；swap 后 running execution 仍用旧 definition；new execution 用新 definition；mutation gate race；lock/permit cleanup。
**验收标准**：active map 包含 agent/installation；Runtime Manager 有 panic-safe per-agent mutation gate；新 execution 不越过 lifecycle 临界区；无 I/O while lock held；`cancel_all` 保留退出用途。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution::executor
```

**依赖任务**：T12。

### T14 移除全局 OnceLock，建立 Runtime Manager Ownership

**状态**：待开始
**目标**：新增 installation-aware Runtime Manager，移除进程级 OnceLock；暂留无全局状态的 deprecated caller shim 到 T15A，以保持增量可编译。
**先读**：`02` §7；`06` §5。
**涉及文件**：

- `src-tauri/src/backend/agent_market/runtime.rs`（new）
- `src-tauri/src/backend/agent_market/mod.rs`
- `src-tauri/src/backend/ai_execution/mod.rs`
- `src-tauri/src/backend/application/service.rs`
- `src-tauri/src/backend/application/system.rs`

**优先编写测试**：两个临时 DB 有独立 Registry；manager 从 repository 发布 ready definitions；failed publish keeps old；shim 不缓存跨 DB 状态。
**验收标准**：无 `SHARED_AGENT_EXECUTION_RUNTIME` static/OnceLock；Manager 是 DB/Registry composition owner；依赖可注入；测试无需 reset global；deprecated shim 不持有全局状态并标注 T15A 删除。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::application::system
cargo test --manifest-path src-tauri/Cargo.toml adapters
cargo check --manifest-path src-tauri/Cargo.toml
```

**依赖任务**：T13。

### T15 迁移业务调用者到显式 Runtime Manager

**状态**：待开始
**目标**：将 Desktop AppState 和四个主要业务/Agent API 调用者迁移到显式 Runtime Manager。
**先读**：`02` §7；前置 ACP SPEC application integration。
**涉及文件**：

- `src-tauri/src/adapters/app_state.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/backend/application/agent.rs`
- `src-tauri/src/backend/application/card_translation.rs`
- `src-tauri/src/backend/application/memory_extraction.rs`

**优先编写测试**：AppState 持有 manager；各 workflow 注入 fake runtime；未 ready error；不同 service 不串 DB。
**验收标准**：除明确留给 T15A 的 Memory Dream 调用外，调用者不再使用 deprecated shim；业务不自行构建 Registry。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::application
rg -n "shared_agent_execution_runtime" src-tauri/src/backend/application src-tauri/src/adapters
```

**依赖任务**：T14。

### T15A 迁移 Memory Dream 并删除 Deprecated Runtime Shim

**状态**：待开始
**目标**：迁移最后一个业务调用者，彻底删除按 DB 临时构建 runtime 的兼容函数。
**先读**：`02` §7；T14/T15 diff；`memory_dream.rs` runtime 调用附近。
**涉及文件**：

- `src-tauri/src/backend/application/memory_dream.rs`
- `src-tauri/src/backend/ai_execution/mod.rs`

**优先编写测试**：Memory Dream 注入 manager/fake runtime；两个 DB 不串；未 ready 稳定错误。
**验收标准**：`rg shared_agent_execution_runtime src-tauri/src` 无生产定义/引用；没有新的 global runtime constructor。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::application::memory_dream
rg -n "shared_agent_execution_runtime|SHARED_AGENT_EXECUTION_RUNTIME" src-tauri/src
cargo check --manifest-path src-tauri/Cargo.toml
```

**依赖任务**：T15。

### Checkpoint C：Runtime

```bash
cargo fmt --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml backend::agents
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution
cargo test --manifest-path src-tauri/Cargo.toml backend::application
```

人工检查：fresh Registry empty、显式 ownership、running execution snapshot、无进程级 Agent OnceLock。

## 9. Phase 5：Lifecycle Coordinator

### T16 实现首装与 System Bind Lifecycle

**状态**：待开始
**目标**：编排 plan -> staging/installer -> conformance -> DB -> Registry -> cleanup。
**先读**：`04` §3–5/9–12；`07` LIFE-01..06。
**涉及文件**：

- `src-tauri/src/backend/agent_market/lifecycle/mod.rs`（new）
- `src-tauri/src/backend/agent_market/lifecycle/install.rs`（new）
- `src-tauri/src/backend/agent_market/mod.rs`
- `src-tauri/src/backend/agent_market/repository.rs`
- `src-tauri/src/backend/agents/registry.rs`

**优先编写测试**：LIFE-01..06；managed conformance fail no row；System failed diagnostic row；activation compensation。
**验收标准**：per-agent lease；无 global app lock；staging never Registry；old snapshot on failure；typed phase/error。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::lifecycle::install
```

**依赖任务**：T03、T07–T13、T15A。

### T17 实现 Update 与 Reinstall

**状态**：待开始
**目标**：新版本/同版本 staged replace，旧版本在激活成功前保持可用。
**先读**：`04` §6–7；`07` LIFE-07..10。
**涉及文件**：

- `src-tauri/src/backend/agent_market/lifecycle/update.rs`（new）
- `src-tauri/src/backend/agent_market/lifecycle/mod.rs`
- `src-tauri/src/backend/agent_market/lifecycle/install.rs`
- `src-tauri/src/backend/agent_market/repository.rs`

**优先编写测试**：download/hash/conformance/DB/Registry failure preserves old；ownership switch；same-version reinstall；catalog fixed version missing。
**验收标准**：manual only；无长期 history；成功后才清 old dir；cleanup failure warning 不回滚可用新版本。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::lifecycle::update
```

**依赖任务**：T16。

### T18 实现 Disable/Enable/Uninstall

**状态**：待开始
**目标**：处理 active execution、assignment conflict、managed 安全删除和 System unbind。
**先读**：`04` §8；`07` §2.8/LIFE-11..15。
**涉及文件**：

- `src-tauri/src/backend/agent_market/lifecycle/uninstall.rs`（new）
- `src-tauri/src/backend/agent_market/lifecycle/mod.rs`
- `src-tauri/src/backend/agent_market/repository.rs`
- `src-tauri/src/backend/app_settings.rs`
- `src-tauri/src/backend/ai_execution/executor.rs`

**优先编写测试**：agent_in_use；assignment preview/confirm；System no delete；unsafe path reject；enable/disable Registry remove/add。
**验收标准**：不自动取消 execution；不自动选择替代 Agent；删除前 canonical ancestor proof。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::lifecycle::uninstall
```

**依赖任务**：T17。

### T19 实现 Startup Recovery

**状态**：待开始
**目标**：清 stale staging、标记 broken、重试 orphan cleanup、加载 Registry，不做网络/ACP probe-all。
**先读**：`04` §13；`07` LIFE-16..18。
**涉及文件**：

- `src-tauri/src/backend/agent_market/lifecycle/recovery.rs`（new）
- `src-tauri/src/backend/agent_market/lifecycle/mod.rs`
- `src-tauri/src/backend/agent_market/repository.rs`
- `src-tauri/src/backend/application/system.rs`

**优先编写测试**：24h staging；missing entry broken；orphan boundary；Registry load；no network/protocol fanout。
**验收标准**：bounded/background capable；不删除不确定目录；启动可继续并输出结构化 warning。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::lifecycle::recovery
```

**依赖任务**：T18。

### Checkpoint D：Lifecycle

```bash
cargo fmt --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::lifecycle
cargo test --manifest-path src-tauri/Cargo.toml backend::agents
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution
```

故障注入审查：每个 phase cancel、update preserves old、System no-delete、active execution conflict、Registry compensation。

## 10. Phase 6：Application、Task、Tauri、Engine、CLI

### T20 实现 Agent Market AppService 与 DTO

**状态**：待开始
**目标**：提供 list/inspect/preview/installed/health 和 lifecycle workflow 业务边界。
**先读**：`06` §2–5；`04` §14。
**涉及文件**：

- `src-tauri/src/backend/application/agent_market.rs`（new）
- `src-tauri/src/backend/application/mod.rs`
- `src-tauri/src/backend/application/service.rs`
- `src-tauri/src/backend/application/agent.rs`
- `src-tauri/src/backend/agent_market/types.rs`

**优先编写测试**：Market merge；preview input ignores arbitrary execution fields；installed health；compat methods；structured errors。
**验收标准**：业务逻辑不在 adapter；路径 display 为 `~`；兼容 API callable；no probe-all list。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::application::agent_market
cargo test --manifest-path src-tauri/Cargo.toml backend::application::agent
```

**依赖任务**：T15、T19。

### T21 扩展 BackgroundTaskRegistry

**状态**：待开始
**目标**：Agent lifecycle/catalog refresh 的 begin/get/list/cancel/finish、retention 和 shutdown snapshot。
**先读**：`04` §2/9/15；现有 `background_tasks.rs` AI/Conversation task patterns。
**涉及文件**：

- `src-tauri/src/adapters/tauri/background_tasks.rs`
- `src-tauri/src/backend/agent_market/types.rs`
- `src-tauri/src/adapters/app_state.rs`

**优先编写测试**：same Agent dedupe；different Agent bounded tasks；terminal merge/retention；cancel；shutdown active count。
**验收标准**：task key tenant/agent；snapshot 无 secret；Agent task 与 AI execution task 分离。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml adapters::tauri::background_tasks
```

**依赖任务**：T20。

### T22 新增 Tauri Commands 与 Events

**状态**：待开始
**目标**：薄 command 启动任务、执行 worker、emit snapshot、提供 polling API 和退出保护。
**先读**：`06` §4；`07` API-02..03/响应性。
**涉及文件**：

- `src-tauri/src/adapters/tauri/agent_market.rs`（new）
- `src-tauri/src/adapters/tauri/mod.rs`
- `src-tauri/src/adapters/tauri/commands.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/adapters/tauri/background_tasks.rs`

**优先编写测试**：start quick snapshot；event phase；poll terminal；cancel；unrelated command while task running；close active report。
**验收标准**：无 global lock across I/O；invoke handler 注册完整；业务委托 AppService/lifecycle。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml adapters::tauri
cargo check --manifest-path src-tauri/Cargo.toml
```

**依赖任务**：T21。

### T23 注册 Engine Methods 并生成 Contract

**状态**：待开始
**目标**：新增 `06` §3 one-shot Engine read/preview/`*.run` 方法、风险/确认元数据，并重新生成 CLI contract。
**先读**：`06` §3；Repository Engine contract rules。
**手写 Files**：

- `src-tauri/src/adapters/engine/registry.rs`
- `src-tauri/src/adapters/engine/contract.rs`（仅实际存在/需要时）
- `src-tauri/src/backend/application/agent_market.rs`

**Generated Files**：

- `cli/internal/schema/contract.json`
- 生成器实际更新的其他 schema artifact（执行前列出）

**优先编写测试**：method exposure、DTO schema、risk/confirmation、兼容 aliases；run 等待终态；context cancel 收敛。
**验收标准**：不手改 generated JSON；Tauri/Engine 共用 lifecycle service；Engine 不暴露不可跨进程轮询的 task ID；contract diff 只含预期方法/DTO。
**验证命令**：

```bash
pnpm cli:contract
cargo test --manifest-path src-tauri/Cargo.toml adapters::engine
go test -C cli ./internal/schema
```

**依赖任务**：T22。

### T24 实现 Go CLI Agent Commands

**状态**：待开始
**目标**：Market/Installed/install/update/reinstall/uninstall/check 命令只调用 one-shot Engine。
**先读**：`06` §6；CLI command/policy conventions。
**涉及文件**：

- `cli/cmd/agent.go`（new）
- `cli/cmd/agent_test.go`（new）
- `cli/cmd/root.go`
- `cli/cmd/metadata.go`（如命令元数据集中维护）
- `cli/cmd/platform_guards.go`（仅 policy 确需）

**优先编写测试**：CLI-01..04；preview/yes/json；单次 run；Ctrl-C；failed/cancelled exit code；Engine method names。
**验收标准**：CLI 无 npm/uv/fs/SQLite；不跨 Engine 进程轮询 task；`--json` stdout 纯 DTO；confirmation policy 正确。
**验证命令**：

```bash
gofmt -w cli/cmd/agent.go cli/cmd/agent_test.go
go vet -C cli ./...
go test -C cli -race ./...
```

**依赖任务**：T23。

### 检查点 E：跨表面 API 验收 (Cross-surface API)

```bash
pnpm cli:contract
cargo test --workspace
go vet -C cli ./...
go test -C cli -race ./...
```

人工检查：Engine/Tauri DTO 一致、CLI 无直接 I/O、risk/confirmation、start 快速返回。

## 11. Phase 7：迁移与 OpenCode 语义

### T25 实现旧 Assignment/Runtime 幂等迁移

**状态**：待开始
**目标**：用 post-upgrade 后台 workflow 只 materialize assigned System Agent；Npx 不静默联网；保留不可用 assignment/model。
**先读**：`05` §6；`06` §10。
**涉及文件**：

- `src-tauri/src/backend/agent_market/migration.rs`（new）
- `src-tauri/src/backend/agent_market/mod.rs`
- `src-tauri/src/backend/app_settings.rs`
- `src-tauri/src/backend/application/system.rs`
- `src-tauri/src/backend/agent_market/repository.rs`

**优先编写测试**：MIG-01、MIG-02、MIG-04..06；assigned/unassigned；System pass/fail；Npx no network；repeat；managed choice preserved。
**验收标准**：SQL migration transaction 不启动进程；no silent install/fallback；assignment/model 保留；启动可观察 notice；Desktop/Engine 共享 workflow；幂等。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market::migration
cargo test --manifest-path src-tauri/Cargo.toml backend::app_settings
```

**依赖任务**：T20。

### T26 修正 OpenCode Connection 语义并移除 `cli_fallback`

**状态**：待开始
**目标**：CLI version 与 ACP connection 分离；实际执行保持 ACP-only。
**先读**：`05` 全文；前置 ACP SPEC D-004。
**涉及文件**：

- `src-tauri/src/backend/agents/types.rs`
- `src-tauri/src/backend/ai_execution/executor.rs`
- `src-tauri/src/backend/application/agent.rs`
- `src-tauri/src/backend/agents/registry.rs`

**优先编写测试**：OC-01..10、MIG-03/07；compat connection method；no `opencode run` spawn。
**验收标准**：`rg cli_fallback` 无生产字段/分支；ACP fail => connected false；Translation 无 CLI execution route。
**验证命令**：

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution
cargo test --manifest-path src-tauri/Cargo.toml backend::application::agent
rg -n "cli_fallback|opencode run" src-tauri frontend cli
```

**依赖任务**：T13、T20、T25。

## 12. Phase 8：Frontend

### T27 扩展 Frontend Agent Service

**状态**：待开始
**目标**：新 DTO、Market/preview/task/installed/health invoke；保留兼容 wrappers。
**先读**：`06` §2/7；frontend service boundary。
**涉及文件**：

- `frontend/src/services/agentRuntime.ts`
- `frontend/src/services/agentRuntime.test.ts`（new 或现有测试文件）
- `frontend/src/services/agents/types.ts`（new，若拆分）

**优先编写测试**：invoke command/params/schema；error；path/display；compat wrapper。
**验收标准**：所有 invoke 在 service；组件无 backend DTO 猜测；uninstalled 无临时 command。
**验证命令**：

```bash
pnpm test -- agentRuntime
pnpm typecheck
```

**依赖任务**：T22、T23。

### T28 实现 AgentLifecycleTaskProvider

**状态**：待开始
**目标**：list/event/poll/cancel/merge/global task state。
**先读**：`06` §8.4；现有 AI/Conversation Providers。
**涉及文件**：

- `frontend/src/app/backgroundTasks/AgentLifecycleTaskProvider.tsx`（new）
- `frontend/src/app/backgroundTasks/AgentLifecycleTaskProvider.test.tsx`（new）
- `frontend/src/app/AppProviders.tsx`
- `frontend/src/services/agentRuntime.ts`

**优先编写测试**：API-03；terminal wins；poll fallback；same Agent active lookup；cancel；listener cleanup。
**验收标准**：与 AiExecution Provider 分离；无 page-level 全局 busy；Provider error 不清空已知 task。
**验证命令**：

```bash
pnpm test -- AgentLifecycleTaskProvider
pnpm typecheck
```

**依赖任务**：T27。

### T29 实现 Market 与 Install Preview UI

**状态**：待开始
**目标**：Agent Settings 的 Market tab、filters、card/detail、distribution preview/confirm/progress。
**先读**：`01` Journey A/B；`06` §8.1–8.2；Product frontend preferences。
**涉及文件**：

- `frontend/src/components/settings/AgentSettingsPanel.tsx`
- `frontend/src/components/settings/AgentMarketView.tsx`（new）
- `frontend/src/components/settings/AgentInstallPreviewDialog.tsx`（new）
- `frontend/src/components/settings/AgentSettingsPanel.test.tsx`
- `frontend/src/components/settings/AgentCatalogIcon.tsx`

**优先编写测试**：UI-01..03；offline catalog；System/Binary choice；preview before start；task progress；current platform incompatible。
**验收标准**：dense workspace style；protocol/distribution分开；不 probe-all；无 Add Custom；只禁用冲突 Agent。
**验证命令**：

```bash
pnpm test -- AgentSettingsPanel AgentMarketView AgentInstallPreviewDialog
pnpm typecheck
```

**依赖任务**：T28。

### T30 实现 Installed 管理 UI

**状态**：待开始
**目标**：Installed tab、三维状态、check/update/reinstall/enable/disable/uninstall preview。
**先读**：`01` Journey C/D；`06` §8.3；`04` 状态模型。
**涉及文件**：

- `frontend/src/components/settings/AgentInstalledView.tsx`（new）
- `frontend/src/components/settings/AgentUninstallDialog.tsx`（new）
- `frontend/src/components/settings/AgentConnectionRow.tsx`
- `frontend/src/components/settings/AgentSettingsPanel.tsx`
- `frontend/src/components/settings/AgentSettingsPanel.test.tsx`

**优先编写测试**：UI-03/05；System no-delete copy；assignment conflict；agent_in_use；degraded health；manual update。
**验收标准**：installed != connected；ownership/status visible；动作调用 preview/service；无静默 replacement。
**验证命令**：

```bash
pnpm test -- AgentSettingsPanel AgentInstalledView AgentUninstallDialog AgentConnectionRow
pnpm typecheck
```

**依赖任务**：T29。

### T31 迁移 Capability Picker 与移除静态目录真相

**状态**：待开始
**目标**：picker 只读 ready installations，保留不可用 current assignment，model 懒加载；静态 catalog 只留纯展示或删除。
**先读**：`06` §9/11；`01` FR-HLT/FR-UX。
**涉及文件**：

- `frontend/src/components/settings/AgentCapabilityDialog.tsx`
- `frontend/src/components/settings/AgentCapabilitySetting.tsx`
- `frontend/src/components/settings/agentCatalog.ts`
- `frontend/src/components/settings/AgentSettingsPanel.test.tsx`
- `frontend/src/components/settings/AgentCatalogIcon.test.tsx`

**优先编写测试**：UI-01/04；only ready candidates；current unavailable + CTA；model lazy；no Promise.all probe-all；no command/package map。
**验收标准**：前端 Agent ID/command/package source of truth 消失；保存错误可恢复；icon metadata 不控制行为。
**验证命令**：

```bash
pnpm test -- AgentCapability AgentSettingsPanel AgentCatalogIcon
pnpm typecheck
rg -n "npx|-y|kiro-cli-chat|cli_fallback" frontend/src/components/settings
```

**依赖任务**：T30。

### Checkpoint F：Frontend

```bash
pnpm typecheck
pnpm test
pnpm build
```

人工验证：Market/Installed、offline、preview、task global visibility、no probe-all、degraded OpenCode copy、assignment preserved。

## 13. Phase 9：清理、E2E 与发布

### T32 删除 Hardcoded Runtime Catalog 与临时 Npx 执行

**状态**：待开始
**目标**：确认新路径稳定后移除旧硬编码 source、`npx -y` runtime definitions 和不再使用的 OpenCode CLI seam。
**先读**：主索引 §4/7；`05` §5.4；`06` §12。
**Files**（执行前用 `rg` 精确收敛，最多 5 个；超出则拆 T32a/b）：

- `src-tauri/src/backend/agents/registry.rs`
- `src-tauri/src/backend/ai_execution/legacy_gemini.rs`（只删除确认未使用部分）
- `frontend/src/components/settings/agentCatalog.ts`
- 相关 colocated test files（最多 2）

**优先编写测试**：CAT-09；no runtime package manager；Gemini legacy 不回归。
**验收标准**：

```text
rg "@agentclientprotocol/|pi-acp|npx.*-y|kiro-cli-chat|cli_fallback"
```

只允许 bundled catalog fixture、历史 migration test 或明确文档引用；生产 execution definition 不允许。
**验证命令**：

```bash
rg -n "npx.*-y|cli_fallback|kiro-cli-chat" src-tauri/src frontend/src cli
cargo test --workspace
pnpm typecheck && pnpm test
```

**依赖任务**：T24、T25、T31。

### T33 全链路验收、文档与进度收口

**状态**：待开始
**目标**：执行质量门、fixture/e2e/manual smoke，更新现状文档和 SPEC 状态。
**先读**：`07` 全文；`10-progress.md`。
**涉及文件**：

- `agent-docs/feature-plans/agent-marketplace-dynamic-runtime/10-progress.md`
- `agent-docs/feature-plans/SPEC_ Agent Marketplace 与动态运行时.md`
- 已淘汰的全局设计总册（以代码、测试与 ADR 为准）（架构已成为事实后）
- 已淘汰的全局需求总册（待办以 GitHub Issues 为准）（产品范围已成为事实后）
- GitHub Issues（已取代文件版任务总册）（Git/测试证据证明后）

**实施步骤**：

1. 运行 `07` Checkpoint A–Final。
2. 运行 System/Binary/Npx/Uvx fixture matrix。
3. 运行 installed/active/update failure/connection truth table e2e。
4. 在支持平台执行 manual smoke 并记录证据。
5. 核对 `git diff` 无手改 generated contract、secret、artifact。
6. 只有全部通过后将状态改为 Implemented。

**验收标准**：Release Acceptance 逐项有 PASS/证据；未通过项保持 Pending，不把目标写成现状。
**验证命令**：

```bash
cargo fmt --all -- --check
cargo test --workspace
go vet -C cli ./...
go test -C cli -race ./...
pnpm typecheck && pnpm test && pnpm build
pnpm cli:test:e2e
git diff --check
```

**依赖任务**：T32。

## 14. 任务执行证据模板

每个 Task 在 `10-progress.md` 记录：

```text
Task: TXX
Status: Complete | Blocked | Pending
Commit: HASH（未提交则写 working tree）
Files changed:
- ...
Tests first:
- TEST_ID: before FAIL reason -> after PASS
Verification:
- COMMAND -> PASS/FAIL, date
Acceptance:
- [x]/[ ] ...
Deviations:
- none / exact deviation + approved ADR
Not touched:
- ...
Next:
- TYY
```

不得以“代码已编译”“看起来正确”代替测试证据。
