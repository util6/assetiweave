# 仓库架构与开发指引 (Repository Guidelines)

## 1. 项目定位与核心红线 (Core Constraints)

- **产品定位**：AssetIWeave 是本地优先（Local-first）的 AI 文件资产挂载管理器，统一管理 Prompt、Rule、Memory、Skill、MCP、Agent 等本地文件资产与会话记录。
- **业务唯一收口**：`src-tauri/src/backend/application/AppService` 是 Tauri 与 Engine 唯一的业务编排边界。前端禁止绕过 `frontend/src/services/` 直调 `invoke(...)`；CLI 禁止绕过 Engine 直接读写 SQLite 或操作文件挂载。
- **持久化真相源**：SQLite 是应用状态、配置与挂载意图（`asset_mounts`）的唯一真相源。数据库变更必须通过 `src-tauri/migrations/`。源资产目录默认只读，严禁向第三方仓库写入应用状态。
- **单层直接软链接**：默认采用从目标 App 目录直连真实源资产的单层直接软链接，禁止私自引入中间软链接池。
- **长任务与并发安全**：目录扫描、大批量挂载、远程拉取、会话同步等 I/O 密集操作必须后台化，禁止在耗时任务期间持有全局应用锁阻塞 UI 或核心读写。

## 2. 模块职责与运行时数据流 (Architecture & Data Flow)

- **桌面端交互流**：`React (UI) -> frontend/src/services -> Tauri Commands -> AppService -> Backend Capabilities/Store -> SQLite & 文件系统`。
- **CLI 执行流**：`Go Cobra -> Engine Client -> stdio JSON Protocol -> AppService -> 共享 Capabilities/Store`。
- **核心模块边界**：
  - `frontend/src/`：React 19 + TypeScript。基础控件位于 `components/foundation` 与 `components/ui`，跨域复用组件在 `components/common`，业务域按 `components/{groups,assets,sources,conversations}` 组织。
  - `src-tauri/src/`：完整 Rust 后端。`adapters/` 负责 Tauri 命令、Engine stdio 协议、系统胶水与后台任务投影；`backend/` 负责业务逻辑、领域模型与存储。
  - `cli/`：Go Cobra 客户端。负责命令行交互、格式化输出与外部采集编排；禁止直接触碰底层数据库与挂载文件。

## 3. 开发、契约与验证命令 (Commands & Baseline)

- **常用命令**：
  - 前端开发/校验：`pnpm dev`（纯前端预览，1420 端口）、`pnpm typecheck && pnpm test && pnpm build`。
  - 桌面端联调：`pnpm tauri:dev`。
  - Rust 校验：`cargo fmt --all -- --check && cargo test --workspace`。
  - Go CLI 校验：`go vet -C cli ./... && go test -C cli -race ./...`。
  - 契约同步：当 Engine 接口、DTO、错误类型或暴露范围变动时，必须执行 `pnpm cli:contract`，严禁手改 `cli/internal/schema/contract.json`。
  - E2E 验证：`pnpm cli:test:e2e`。
- **环境基线**：Node 22, pnpm 10, Go 1.24, Rust 1.96.0+。

## 4. 架构规范与开发模式 (Engineering Patterns)

- **代码与设计规范**：
  - TypeScript 采用 2 空格、分号、双引号；组件/类型使用 `PascalCase`，函数/变量/hooks 使用 `camelCase`（hook 加 `use` 前缀）。
  - Rust 遵守 `rustfmt`，Go 遵守 `gofmt`。
  - UI 颜色、边框与阴影必须使用语义化 Theme Token 或 Foundation 基础组件，严禁硬编码原始色值。
  - 参考 Cockpit-tools、VS Code、Finder 的高密度工作区设计（侧边栏、工具栏、可调列宽、操作预览同屏呈现），避免多级弹窗阻断操作。
- **长耗时功能规范**：
  - 长任务禁止直接在按钮点击中 `await` 全过程；后端命令应快速返回任务快照（Snapshot），耗时处理委托后台任务。
  - 前端必须通过统一 Provider 管理任务状态，结合事件订阅与轮询兜底（Polling fallback），展示全局/局部进度。
  - 批量操作必须在前端完成去重，共享数据一次性装载，完成时触发单次聚合刷新。任务进行中仅禁用冲突操作，保留无关浏览与查看能力。
- **提交规范**：
  - 采用 Conventional Commits（如 `feat: add source filter` 或 `fix: refresh mount state`），保持原子提交。禁止提交临时日志、秘密凭证与编译产物。

## 5. 事实源与文档规范 (Source of Truth)

- **权威性优先级**：代码与测试事实（代码、测试、CLI `--help`） > 规划事实（GitHub Issues、Agent Briefs） > 术语字典（`CONTEXT.md`） > 架构决策（`agent-docs/adr/`） > 用户文档（`docs/`）。
- **目录隔离**：
  - `docs/`：存放用户手册与可长期沉淀的系统知识。
  - `agent-docs/`：存放 Agent 治理规范（`governance/`）、架构决策（`adr/`）、专项实施计划与工作过程材料（`feature-plans/`）。
- 保持单一事实源，避免多处维护副本；产出设计若与已有 ADR 冲突，必须显式说明理由。

## 6. Agent 治理与专项路由 (Governance & Agent Skills)

### 问题追踪器 (Issue Tracker)

问题与规格均通过本仓库的 GitHub Issues 跟踪管理。详见 `agent-docs/governance/issue-tracker.md`。

### 分诊标签 (Triage Labels)

使用标准五个分诊状态角色（`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`）。详见 `agent-docs/governance/triage-labels.md`。

### 领域文档 (Domain Docs)

本仓库采用单上下文文档布局。详见 `agent-docs/governance/domain.md`。

### 活跃专项执行路由 (Active Feature Routers)

- **Memory 架构重写 (Issue #20)**：涉及 Recent Work、Session/Project/Global Memory、Context Resolver、Recall Agent 或旧版 Memory 切换时，先读取 `agent-docs/feature-plans/memory-rewrite/00-execution-router.md`。
- **后端基础设施收口 (Issue #24)**：涉及 Database Runtime、Event Dispatcher、HostProcess、后端 Settings、tracing、路径契约或 SQLx row 收口时，先读取 `agent-docs/feature-plans/backend-infrastructure-convergence-v2/00-execution-router.md`。
