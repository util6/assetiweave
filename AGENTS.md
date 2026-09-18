# 仓库架构与开发指引 (Repository Guidelines)

## 1. 项目定位与核心红线 (Core Constraints)

- **产品定位**：AssetIWeave 是本地优先（Local-first）的 AI 文件资产挂载管理器，统一管理 Prompt、Rule、Memory、Skill、MCP、Agent 等本地文件资产与会话记录。
- **业务唯一收口**：`src-tauri/src/backend/application/AppService` 是 Tauri 与 Engine 唯一的业务编排边界。前端禁止绕过 `frontend/src/services/` 直调 `invoke(...)`；CLI 禁止绕过 Engine 直接读写 SQLite 或操作文件挂载。
- **持久化真相源**：SQLite 是应用状态、配置与挂载意图（`asset_mounts`）的唯一真相源。数据库变更必须通过 `src-tauri/migrations/`。源资产目录默认只读，严禁向第三方仓库写入应用状态。
- **单层直接软链接**：默认采用从目标 App 目录直连真实源资产的单层直接软链接，禁止私自引入中间软链接池。
- **长任务与并发安全**：目录扫描、大批量挂载、远程拉取、会话同步等 I/O 密集操作必须后台化，禁止在耗时任务期间持有全局应用锁阻塞 UI 或核心读写。
- **前端视觉规范红线**：所有页面与组件必须严格遵循 ADR-0009 与 `agent-docs/governance/frontend-design-system.md` (Auroraqua-UI)。严禁自由发挥、严禁在交互控件上使用 `rounded-sm`/`rounded-md` 等生硬方角破坏流体质感、严禁 Ad-hoc 拼凑未经设计系统收口的按钮与输入框。新 UI 必须以“对话记录”与“Skill 目录总览”为黄金参考系。

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
  - **组件分层架构与引入**：严格遵守 Foundation (`components/foundation/` 与 `components/ui/`) -> Common (`components/common/`) -> Domain (`components/{domain}/`) 三层架构。业务层严禁手写裸 HTML 按钮、输入框与自定义遮罩弹窗。
  - **设计系统与视觉规范 (Auroraqua-UI)**：
    - 统一采用温润象牙/暗色玻璃拟态、微光辉与内阴影高光（Theme Tokens & Recipes）。严禁硬编码原始 Hex/RGB/HSL 色值或使用 Tailwind 默认调色盘类名。
    - **圆角标尺**：坚决杜绝方角硬边。卡片与面板使用 `rounded-xl`/`rounded-2xl`，按钮与工具栏使用 `rounded-xl`/`rounded-2xl`，徽章与分段切换器强制使用胶囊 `rounded-full`。严禁在控件上使用 `rounded-sm`/`rounded-md`。
    - **分段切换与标签栏**：一律使用 `PillTabs` (`components/common/PillTabs`) 或 `aurora-pill-tab` 规范，享受流体滑动的 `aurora-pill-indicator` 与温润微光；严禁对 Tab 激活项误用按钮级突兀渐变 (`theme-primary-gradient`)。
    - **弹窗与底栏**：弹窗必须使用 `DialogFrame`；底栏按钮默认保持标准 `h-10` 饱满高度（主按钮 `variant="default"`，次按钮 `variant="outline"`），严禁缩减为干瘪的小方块。
  - 参考 Cockpit-tools、VS Code、Finder 的高密度工作区设计（侧边栏、工具栏、可调列宽、操作预览同屏呈现），避免多级弹窗阻断操作。
- **Rust 测试代码组织与业务文件瘦身规范（测试拆分是主线）**：
  - **测试拆分是主线**：新增 unit test 严禁内联写入业务实现文件，必须默认与业务实现分离到同级独立文件（如 `session.rs` 对应 `session_tests.rs`）。
  - **标准挂载模式**：业务文件末尾仅保留模块声明：
    ```rust
    #[cfg(test)]
    #[path = "<filename>_tests.rs"]
    mod tests;
    ```
    `tests` 保持作为原模块直接子模块，使用 `use super::*;` 访问私有项；严禁为了测试拆分而人为扩大业务 API 可见性（不要把原本 private 改成 `pub`）。
  - **命名与结构镜像**：普通文件 `xxx.rs` 对应 `xxx_tests.rs`；目录模块 `dir/mod.rs` 对应 `dir/<dir>_tests.rs`。测试辅助代码（fixtures/mocks/builders）如仅本模块使用放对应 `*_tests.rs`，多处复用抽到 `test_support`。
  - **文件规模硬性红线**：单个业务实现文件（不计测试代码）严格控制在 **500 行以内**；当业务实现接近或超过 **800 行** 时必须优先拆分出新的子业务模块，严禁以剥离测试为借口继续在原文件膨胀业务代码。
  - **演进策略（童子军规则）**：存量轻量小文件（< 500 行）无需全库机械刷库；但在功能迭代被实质修改、测试继续增长或接近规模红线时，必须顺带将其测试迁移至同级独立 `*_tests.rs`。
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

### 前端设计系统 (Frontend Design System)

前端界面统一采用 Auroraqua-UI 设计体系与组件三层架构，以“对话记录”与“Skill 目录总览”为黄金参考系。详见 `agent-docs/governance/frontend-design-system.md`。

### 活跃专项执行路由 (Active Feature Routers)

- **Memory 架构重写 (Issue #20)**：涉及 Recent Work、Session/Project/Global Memory、Context Resolver、Recall Agent 或旧版 Memory 切换时，先读取 `agent-docs/feature-plans/memory-rewrite/00-execution-router.md`。
- **后端基础设施收口 (Issue #24)**：涉及 Database Runtime、Event Dispatcher、HostProcess、后端 Settings、tracing、路径契约或 SQLx row 收口时，先读取 `agent-docs/feature-plans/backend-infrastructure-convergence-v2/00-execution-router.md`。
- **统一任务中心与并发同步 (Issue #33)**：涉及 Task Center、TaskRuntime 阶段/活动/保留、Conversation Adapter 分组并发、Adapter 结构化进度、气泡通知或任务设置时，先读取 `agent-docs/feature-plans/task-center-and-sync/00-execution-router.md`。
- **共享 Agent Session 与 AionUi 聊天工作区 (Issue #31)**：涉及 Agent Session View、Team 聊天/并行成员、Thinking/Tool Steps、Memory Agent 执行现场或旧 Team Session 呈现收缩时，先读取 `agent-docs/feature-plans/agent-session-workspace/00-execution-router.md`。
