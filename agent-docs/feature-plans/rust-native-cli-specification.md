# SPEC: Rust Native CLI 全量迁移与无 GUI 纯服务器独立运行体系

## Problem Statement

当前系统的命令行工具 `aiwc`（位于根目录 `cli/`）是早期使用 Go Cobra 编写的独立客户端。随着后端技术栈在 Rust 上的深度固化与架构收口（包括 ACP 协议微内核与统一任务中心），现有 Go CLI 暴露出了多重系统性架构缺陷：
1. **双编译工具链冗余**：开发与 CI/CD 必须同时维护 Rust 1.96+ 与 Go 1.24 两套沉重工具链，构建矩阵复杂度高。
2. **双二进制版本脱节风险**：Go CLI 每次执行都依赖外部拉起 `assetiweave-engine` Rust 子进程，频繁面临 `engine_migration_outdated`、找不到 Engine 或子进程通信被外部中断的脆弱性。
3. **冷启动与 IPC 开销**：每次敲击命令均需跨进程 stdio JSON 序列化，且依赖脆弱的 `contract.json` 代码生成机制。
4. **业务逻辑严重泄露**：WebHarvester（Chrome Keychain 解密、LevelDB 读取）在 Go CLI 内部“野生生长”，导致桌面端 Tauri 无法复用该能力，违反 ADR-0004 单一业务权威原则。
5. **服务器与现代交互受限**：在无 GUI 的纯服务器系统上，缺乏原生一体化的工作区控制体验，且现有结构难以向全屏 TUI（终端用户界面）平滑演进。

## Solution

在 `backend/` 下建立与 `asset_core`、`agent_engine` 对等同级的纯 Rust Native Crate `backend/cli`（对外交付产物名称仍为 `aiwc`）。
1. **单一二进制与进程内直连**：CLI 直接在进程内以 `OneShot` 模式链接 `asset_core` 的 `AppService`，消除所有子进程启动、stdio 管道以及 `contract.json` 生成环节，实现微秒级本地响应。
2. **采集核心彻底下沉**：借重构契机将 Web 采集与 Harvester 管理逻辑彻底下沉至 `backend/asset_core`，成为桌面端与 CLI 共享的领域能力。
3. **四层单向流与 TUI 演进架构**：内部划分为 `Parser (clap)` -> `Service Binding` -> `ViewModel (纯数据结构)` -> `Renderer (comfy-table / JSON)` 四层。ViewModel 层零外部终端依赖，为未来接入 `ratatui` 全屏交互预留天然 Seam。
4. **无 GUI 纯服务器一等公民**：资产挂载、技能管理、会话检索、SQLite 存储与配置治理等所有核心能力在纯无头 Linux 终端 100% 独立稳定运行；针对缺少桌面浏览器的环境友好降级。
5. **平滑切流与双轨回归**：重构期间根目录旧 Go `cli/` 保持完全不动，通过 Rust 原生集成测试与现有 Go E2E 黑盒跨语言比对，全绿后一键切流。

## User Stories

1. 作为开发者，我希望运行单一二进制 `aiwc` 即可直接完成所有工作区操作，以便不再需要单独构建和寻找 `assetiweave-engine`。
2. 作为运维或 DevOps，我希望在 CI/CD 中只依赖单一 Rust 工具链即可构建桌面端与 CLI，以便大幅缩短镜像拉取与编译时间。
3. 作为纯服务器/远程虚拟机用户，我希望在没有 X11/Wayland 桌面环境的 Linux 终端中运行 `aiwc`，以便完全管理我的 AI 资产与技能挂载。
4. 作为纯服务器用户，我希望在终端中通过清晰的表格和状态高亮查看技能与资产列表，以便获得媲美 GUI 界面的阅读体验。
5. 作为 AI Agent 开发者，我希望运行 `aiwc ... --format json` 时输出的 JSON 结构与旧版本 100% 保持一致，以便现有的 `assetiweave-memory` skill 和自动化脚本无需任何修改即可正常运行。
6. 作为用户，我希望运行 `aiwc version` 时能够瞬时（毫秒级）返回版本信息，以便进行环境探活时无需等待数据库连接池初始化。
7. 作为用户，我希望运行 `aiwc overview` 时能看到已注册源、已索引资产、Profile 与最后扫描状态的聚合概览，以便一目了然当前工作区规模。
8. 作为用户，我希望运行 `aiwc skill list` 时能按租户列出全部已收纳的技能及其挂载状态，以便快速定位需要的技能。
9. 作为用户，我希望运行 `aiwc skill mount <skill_id> --to <target_app>` 时建立单层直接软链接，以便目标 AI 工具立即识别新技能。
10. 作为用户，我希望运行 `aiwc skill unmount <skill_id> --from <target_app>` 时精准移除指定软链接，以便清理目标工作区且不残留无效链接。
11. 作为用户，我希望运行 `aiwc skill group list` 与 `aiwc skill group mount <group_id>` 时能批量挂载场景化技能组，以便快速切换编程或写作场景。
12. 作为用户，我希望运行 `aiwc skill backup` 时能将当前技能库完整归档，以便在环境迁移或重置时快速恢复。
13. 作为用户，我希望运行 `aiwc source list` 时查看当前所有被纳管的资产源目录及其类型（本地目录或 Git 仓库），以便理清资产来源。
14. 作为用户，我希望运行 `aiwc source add <path>` 与 `aiwc source remove <id>` 时能便捷纳管新的技能仓库，以便扩充本地资产池。
15. 作为用户，我希望运行 `aiwc source scan` 时能触发增量文件哈希与元数据索引，以便及时识别上游仓库的资产变动。
16. 作为用户，我希望运行 `aiwc profile list` 时查看所有受支持的 AI 应用程序（如 Cursor, Claude Code, VS Code, Windsurf）及其检测状态，以便了解哪些工具可挂载。
17. 作为用户，我希望运行 `aiwc tenant list` 与 `aiwc tenant switch <id>` 时能隔离不同的租户/团队上下文，以便在个人与工作资产间自由切换。
18. 作为用户，我希望运行 `aiwc conversation search --query <keyword>` 时能快速检索历史对话卡片与代码摘要，以便为终端会话召回上下文。
19. 作为用户，我希望运行 `aiwc conversation sync` 时能增量同步各适配器支持的本地客户端对话，以便卡片库保持最新。
20. 作为用户，我希望运行 `aiwc conversation adapter list` 时能查看已安装的对话解析适配器与运行时健康状态，以便诊断解析能力。
21. 作为用户，我希望运行 `aiwc harvester list` 与 `aiwc harvester run` 时能够运行下沉至后端的本地会话采集器，以便归档网页端 AI 会话。
22. 作为纯 Linux 服务器用户，当我在无桌面浏览器环境下运行网页采集命令时，我希望收到友好的无桌面环境指引，而不是晦涩的底层加解密崩溃。
23. 作为系统管理员，我希望当多个终端命令并发执行时，SQLite 数据库自动排队重试而不是报错锁死，以便自动化并发脚本稳定执行。
24. 作为终端用户，当发生参数错误或业务异常时，我希望看到清晰的错误原因与修复建议（Hint），以便立即自行修正。
25. 作为终端用户，当未来系统升级引入全屏 TUI 交互界面时，我希望原有子命令继续兼容无损，以便脚本工作流不受任何冲击。

## Implementation Decisions

### 1. 架构模块划分与同级三足鼎立
- 在 `backend/` 下创建独立的 `backend/cli` Crate，与 `backend/asset_core` 和 `backend/agent_engine` 构成同级 Workspace 成员。
- 包名设为 `cli`，二进制声明为 `aiwc`。
- 依赖关系严格为单向：`backend/cli` 依赖 `asset_core` 和 `agent_engine`；核心业务 Crate 绝不反向依赖 CLI。

### 2. 进程内直调（In-Process）与单次运行时生命周期
- 废除原有的跨进程 Stdio JSON-RPC 模式；
- CLI 在启动时通过 `asset_core::application::AppService::open_for_engine().await` 以 `RuntimeRole::OneShot` 模式加载环境配置与 SQLite 连接池；
- 确保 SQLite 连接配置了 WAL 模式与 `busy_timeout = 5000ms`，保障多进程并发访问的韧性。

### 3. WebHarvester 领域能力下沉
- 将原 `cli/internal/webharvester` 与 `cli/internal/harvesters` 抽象为纯 Rust 领域服务，下沉至 `backend/asset_core/src/domain/conversations/harvester/`；
- 剥离与 CLI 展现强相关的逻辑，保留存储解析、模板渲染与沙箱调度，使 Tauri 桌面端与 CLI 获得完全等价的采集能力；
- 针对跨平台桌面密钥环（macOS Keychain / Windows DPAPI），加入运行环境探测，纯服务器无头环境自动停用浏览器本地解密，输出指导提示。

### 4. 内部四层解耦（TUI-Ready Architecture）
`backend/cli` 内部严格遵守四层单向数据流：
- **Parser (`args/`)**：使用 `clap` (derive 风格) 声明子命令、全局参数与环境推导；
- **Service Binding (`service/`)**：负责解析参数、调用 `AppService` 方法并做业务异常拦截；
- **ViewModel (`view_models/`)**：纯 Rust 结构体，不依赖终端着色与打印库，作为业务数据向展示层传递的统一标准；
- **Renderer (`render/`)**：负责根据 `--format` 分流：
  - `OutputFormat::Json`：使用 `serde_json::to_string_pretty`，输出字段与原 Go CLI 100% 保持一致；
  - `OutputFormat::Table`：使用 `comfy-table` 排版，搭配 ANSI 微光色彩；
  - 未来引入 `ratatui` 时仅需在 `render/tui` 下挂载新展示模块，零修改业务与 ViewModel 层。

### 5. 迁移期间物理与流程隔离
- 迁移全程严禁修改或移动根目录的 `cli/` 目录；
- CI 工作流和现有发布流程不受任何破坏；
- 全量命令在 `backend/cli` 中验证全绿后，通过单一原子提交完成构建脚本切流与旧目录清理。

## Testing Decisions

### 1. 唯一的最高测试接缝：进程级黑盒调用（Process Invocation Seam）
- 测试统一在系统最外层边界进行验证：**调用编译生成的 `aiwc` 二进制，传入标准命令行参数/环境变量，断言 stdout 格式、stderr 诊断信息与进程退出码**。
- 测试外部可见的系统副作用：断言 SQLite 数据库相关表行数与文件系统软链接树的真实状态。
- **优点**：测试仅验证外部契约与真实行为，与内部模块实现细节解耦，架构重构或向 TUI 升级时测试无需重写。

### 2. 双轨自动化验证矩阵
- **Rust 原生集成测试 (`backend/cli/tests/`)**：
  使用 `assert_cmd` / `tempfile` 构建针对每个子命令的端到端集成测试，覆盖从参数解析、错误提示到真实挂载的全链路。
- **Go E2E 黑盒反向回归**：
  直接复用仓库现存的成熟资产 `cli/tests/cli_e2e/cli_e2e_test.go`，将测试执行目标设为 `target/debug/aiwc`，自动化跑通整套基准测试，确保两套实现的外部行为 100% 同构。

### 3. 先行参考用例
- 参考 `cli/tests/cli_e2e/cli_e2e_test.go` 中针对 `skill mount` 互斥挂载、`overview` 获取、`source scan` 的完整断言模式。

## Out of Scope

1. **本期不包含全屏交互式 TUI 视图**：本期重点在于完成全量命令在命令驱动终端下的原生 Rust 移植，全屏 TUI 作为后续演进目标。
2. **纯无头 Linux 不做桌面浏览器假解密**：在无 GUI Linux 服务器上不伪造桌面浏览器的 Keychain 依赖，明确提示该能力仅限本地桌面浏览器环境。
3. **不增加后台常驻 Daemon 进程**：保持轻量无状态的单次执行模型，不额外引入常驻后台进程以避免守护进程管理复杂度。

## Further Notes

- 编译生成的二进制文件名字统一命名为 `aiwc`，保障用户手感与现存所有 AI 自动化 Agent（如 Antigravity `assetiweave-memory` skill）对底层二进制调用的完全透明兼容。
