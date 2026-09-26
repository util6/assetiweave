# 0016: 后端采用单向领域分层架构与严格模块边界

> **状态**：已接受
> **决策日期**：2026-09-22
> **产品规格**：[Issue #44](https://github.com/util6/assetiweave/issues/44)
> **取代**：旧有扁平混合目录、`capabilities` 能力门面、顶层 `runtime/events/extension_kernel` 混合模块

## 背景

AssetIWeave 后端随业务演进，形成了多种异构的组织模式：
1. 部分业务逻辑分散在 `backend/capabilities/` 粗粒度门面之后，弱化了业务领域的边界与职责收口。
2. 基础能力（如 `runtime`、`events`、`extension_kernel`、`data_backup`）直接平铺于 `backend/` 顶层，与业务上下文混杂，且存在从底层基础设施反向依赖业务层 `AppService` 的倒挂调用。
3. 领域核心实体、纯计算逻辑、SQL 持久化、后台任务与协议适配存在跨层交叉引用，缺乏纯粹的领域不变量层。
4. 随着单文件代码膨胀，部分文件超过千行，测试与业务实现混杂，缺乏同级测试分离与模块规模治理标准。

## 决策

1. **五层单向架构**：
   后端严格遵守单向依赖架构：
   ```text
   Adapters -> Application (AppService) -> Domain
                            |               ^
                            +-> Store ------+
                            +-> Infrastructure
   ```
   - **Adapters** (`src-tauri/src/adapters/`)：负责 Tauri IPC、Engine stdio 协议解析与 Wire 映射，不直调底层存储或文件系统。
   - **Application** (`src-tauri/src/backend/application/`)：以 `AppService` 为业务唯一收口，编排六大业务域（`catalog`, `mounting`, `conversations`, `memory`, `agents`, `system`），协调事务、副作用与后台任务。
   - **Domain** (`src-tauri/src/backend/domain/`)：纯业务实体、值对象、状态机与领域规则；无外部 I/O、SQLx、Tauri、文件系统或网络依赖。
   - **Store** (`src-tauri/src/backend/store/`)：按领域组织持久化仓储（`catalog`, `mounting`, `conversations`, `memory`, `system`）；依赖纯 Domain，禁止反向依赖 Application 与 Adapters。
   - **Infrastructure** (`src-tauri/src/backend/infrastructure/`)：提供跨业务的基础设施能力（`runtime`, `tasks`, `events`, `extensions`, `host_process`, `backup`, `app_settings` 等）；禁止反向依赖 Application。

2. **彻底移除历史兼容门面**：
   - 彻底删除 `backend/capabilities`、`backend/events`、`backend/extension_kernel` 等旧兼容目录。
   - 领域事件系统拆分为：Domain（业务事实 `domain/conversations/events.rs`）、Store（可靠存储 `store/system/{outbox_repo,consumer_offset_repo}.rs`）、Infrastructure（调度投递 `infrastructure/events/`）、Application（业务消费者 `application/conversations/event_handlers.rs`）。

3. **规模红线与测试拆分**：
   - 单个业务实现文件（不计测试代码）严格控制在 500 行以内；接近或超过 800 行时必须拆分子模块。
   - 单元测试默认剥离至同级 `*_tests.rs` 独立文件，通过 `#[cfg(test)] #[path = "..._tests.rs"] mod tests;` 标准挂载，不污染生产代码。

4. **架构守卫脚本自动化**：
   - 建立并维护 `scripts/check-module-boundaries.sh`，强制执行反向依赖检测、旧路径零容忍检测、运行时桥接（`block_on`/`run_sync`）零容忍检测。

## 备选方案

### 保持现状并通过命名约定弱约束
- 缺点：随着代码演进，依赖倒挂和超大文件必然死灰复燃，缺乏自动化门禁保障。结论：否决。

### 拆分为独立的 Cargo Workspaces / Crates
- 缺点：过度工程化，大幅增加编译期抽象开销与包版本管理复杂度，对于当前单一桌面与 CLI 二进制架构并不划算。结论：否决，采用单 crate 内部严格目录与模块可见性收口。

## 后果

- 模块依赖清晰、单向可溯，任何破坏分层（如底层反向 import 上层）都会在 CI/守卫脚本中即刻阻断。
- 业务测试运行更快、目标更明确；业务实现轻量直观，可维护性显著提升。
- 对外 Engine 契约与 CLI 交互接口保持 100% 零漂移兼容。
