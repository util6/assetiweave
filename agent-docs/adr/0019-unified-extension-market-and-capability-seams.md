# 0019: 统一插件市场三表模型、能力契约接缝与 ACP 引擎边界收口

> **状态**：已接受
> **决策日期**：2026-10-31
> **演进收敛**：收敛并修正 [ADR-0011](./0011-on-demand-acp-agent-marketplace.md) 与 [ADR-0012](./0012-extension-kernel-and-runtime-refactor.md) 中未彻底落地的双市场分裂表结构与半成品模块边界

---

## 背景

在 [ADR-0011](./0011-on-demand-acp-agent-marketplace.md) 与 [ADR-0012](./0012-extension-kernel-and-runtime-refactor.md) 之后，系统虽然在概念上提出了 `Extension Kernel`（扩展内核），但在物理实现上仍处于“半成品与历史遗产并存”的分裂状态：

1. **数据库表结构两套体系 + 4 张僵尸旧表并存（共 8 张表）**：
   - 2026-08-29 迁移创建了 `app_agent_installations`（1 张 31 列宽表）与 `app_conversation_adapter_*`（3 张表），但未清理 `agent_installations`、`conversation_adapter_packages`、`conversation_adapter_package_versions`、`conversation_adapter_catalog_releases` 4 张无前缀旧表。
   - ACP Agent 与对话适配器（Conversation Adapter）同属“从市场拉取目录 $\rightarrow$ 下载校验 $\rightarrow$ 版本化解压 $\rightarrow$ 探活激活”的外部桥梁插件，却维护着两套截然不同的持久化表，且 ACP 缺少多版本留存与数据库级目录发布缓存（仍依赖 `~/.assetiweave/cache/agent-market/*.json` 零散文件）。
2. **`agent_engine` 与 `asset_core` 职责边界残留越界**：
   - `backend/agent_engine` 定位为零数据库依赖的强 AI 执行中台，但 `asset_core/src/infrastructure/agent_market/` 中仍残留了 `probes_acp.rs`、`probes_acp_health.rs`、`materialize.rs` 等纯 ACP 协议探活与运行时物化逻辑，同时混杂着通用的包下载与解压器。
3. **现阶段产品开发效率与未来全微内核演进的平衡**：
   - 现阶段（VS Code 模式）仅需将对接外部智能体生态的桥梁组件（ACP Agent、对话适配器）放入插件市场独立分发；而核心业务功能（如 Memory 记忆流水线、Team Agent 多智能体协同）仍需在主程序内高速迭代，但必须在架构上预留平滑演进至 DeepSeek Harness（Cordis）式全微内核的接缝。

---

## 决策

### 1. 确立「外部桥梁插件市场 + 能力契约预埋微内核」双阶段架构

- **能力契约（Capability Contract）作为唯一交互边界**：
  - 宿主核心业务（Memory、Team Agent 等）作为内置模块开发时，严禁硬编码耦合具体外部 Agent 实现，统一通过标准能力契约（如 `AgentExecutionRuntime`）与领域事件总线（`DomainEvent`）协同，且注册项绑定所属模块标识以支持未来的副作用回滚（Effect Unwind）。
- **插件市场（Extension Market）聚焦外部桥梁插件**：
  - 现阶段通过插件市场下载、安装与治理的对象收敛为外部桥梁插件（提供 `agent.acp` 能力的 ACP 智能体包，以及提供 `conversation.adapter` 能力的对话适配器包）。

### 2. 数据库一步到位收敛为统一插件市场三表模型（8 张表 $\rightarrow$ 3 张表）

在一条新的 14 位 SQLx 迁移中，将现有 `app_agent_installations` 与 `app_conversation_adapter_*` 数据无损迁入以下 **3 张统一表**，并一次性 `DROP` 全部 8 张新旧分裂表（4 张无前缀僵尸表 + 4 张旧 `app_*` 分裂表），同时废除 `~/.assetiweave/cache/agent-market/*.json` 本地缓存文件：

1. **`app_extensions`（已安装插件主表）**：
   - 公共生命周期列：`extension_id` (PK)、`capability_kind`（如 `'agent.acp'`、`'conversation.adapter'`；不设封闭二选一 `CHECK` 约束，以便未来微内核阶段直接扩展 `'memory.provider'`、`'team.orchestrator'` 等能力类型）、`target_id`（对应原 `agent_id` 或 `adapter_id`，建 `UNIQUE(capability_kind, target_id)` 索引）、`display_name`、`active_version`、`distribution_type`（`'binary' | 'npx' | 'archive' | 'workspace'`）、`install_dir`、`enabled`、`installation_status`（`'ready' | 'incompatible' | 'broken'`）、`runtime_status`、`error_code`、`error_message`、`checked_at`、`installed_at`、`updated_at`。
   - 能力专有载荷列：`capability_meta_json TEXT NOT NULL CHECK (json_valid(capability_meta_json))`，按 `capability_kind` 承载类型化扩展属性（如 `agent.acp` 的 `resolved_program`、`args`、`definition_json`、`protocol_status`、`model_status`；`conversation.adapter` 的 `record_kind`、`manifest_path`、`adapter_manifest_path`、`origin`、`update_policy` 等）。
   - 存量历史 `protocol = 'native'` 或 `distribution_type IN ('system', 'uvx')` 记录在迁入时自动降级标记为 `installation_status = 'incompatible'`、`error_code = 'agent_reinstall_required'`。
2. **`app_extension_versions`（插件已安装多版本表）**：
   - 主键 `(extension_id, version)`，包含 `install_dir`、`artifact_sha256`、`content_hash`、`verification_status`、`installed_at`（外键级联 `app_extensions`），赋予 ACP Agent 与对话适配器同等的多版本留存与回滚能力。
3. **`app_extension_catalog_releases`（统一插件市场目录与版本缓存表）**：
   - 主键 `(catalog_source, extension_id, version)`，包含 `capability_kind`、`target_id`、`display_name`、`publisher`、`channel`、`released_at`、`artifact_url`、`artifact_size`（可空）、`artifact_sha256`（可空）、`release_meta_json`、`fetched_at`。

### 3. 物理搬空 `asset_core/src/infrastructure/agent_market/`，明确双 Crate 边界

彻底删除 `backend/asset_core/src/infrastructure/agent_market/` 目录，按职责一分为二：
- **下沉至 `backend/agent_engine`（纯 ACP 引擎中台，零数据库依赖）**：
  - 迁入 `probes_acp.rs`、`probes_acp_health.rs`、`runtime/definition.rs`、`runtime/model_cache.rs` 及 `materialize.rs` 中的 ACP 运行时物化逻辑（删除 `probes_native.rs` 与 Native 执行分支）；移除 `asset_core` 对 `agent-client-protocol` 的直接依赖。
- **收口至 `backend/asset_core/src/infrastructure/extensions/`（扩展内核与通用包治理）**：
  - 迁入通用包下载与归档解压（`binary` / `npx`，删除 `system` 与 `uvx` 安装器）、版本目录布局（`layout.rs`，含 `cleanup_runtime_directories`）、包清单校验（`manifest.rs`）与上游版本探测（`upstream_probe.rs`）。
- **保持 `AppService` 对外门面兼容**：
  - `AppService` 编排层内部统一读写 `app_extensions` 三表与 `extensions` 内核，对外保持现有 Tauri Commands 与 CLI 命令签名兼容。

### 4. SQLx 迁移版本号全量统一为 14 位

- 将 `backend/asset_core/migrations/` 下全部 76 个 12 位历史迁移文件统一重命名为 14 位（`YYYYMMDDNNNN` $\rightarrow$ `YYYYMMDD00NNNN`）。
- 在 `open_migrated_pool` 执行 `MIGRATOR.run` 之前前置执行 `_sqlx_migrations` 版本号自愈映射（将 `< 10000000000000` 的 12 位版本号原地转换为 14 位），并通过契约测试锁死 14 位命名规范与 `app_extension_*` 表白名单。
