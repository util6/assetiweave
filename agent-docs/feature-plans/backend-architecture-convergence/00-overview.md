# SPEC-BA-00：后端架构收口总纲

- 状态：Implemented；当前代码与自动化验收已完成，历史基线记录保留
- 日期：2026-08-20
- 代码基线：`main@bc5c14e`
- 适用范围：`src-tauri/src/`、`frontend/src/services/`、Agent 设置界面、`cli/` 契约、`builtin-assets/`、CI 与发布脚本
- 目标读者：后续直接执行代码修改的工程师和代码模型
- 上位约束：仓库根目录 `AGENTS.md`、已淘汰的全局需求总册（待办以 GitHub Issues 为准）、已淘汰的全局设计总册（以代码、测试与 ADR 为准）
- 关联规范：`runtime-extension-refactor/00-08`、0010、0011

> 行号只用于说明 2026-08-20 的审计证据。执行时 MUST 通过符号名重新定位，不得依赖固定行号。

本文是对当前代码基线的“收口执行增量”，不删除原 runtime-extension-refactor 规范。发生冲突时：

1. `AGENTS.md` 和已接受 ADR 优先。
2. 本套 SPEC 对 2026-08-20 后的迁移缺口与 Agent Market 事故给出更具体要求，优先于旧分册的
   同主题基线计数和完成判断。
3. 本套未覆盖的 Domain Event、Conversation Catalog v2 等既有约束继续按原分册执行，并由
   SPEC-BA-09 固化防回退项。

## 1. 规范用语

- **MUST / MUST NOT**：强制要求，违反即视为任务未完成。
- **SHOULD / SHOULD NOT**：默认要求；偏离时必须在 PR 中写明理由、影响与补偿测试。
- **MAY**：可选，不得成为其他强制项的隐式前提。
- **Authority**：某类状态、规则或行为的唯一事实源。
- **Projection**：从 Authority 派生的展示模型，不得独立决定事实状态。
- **Compatibility seam**：迁移期兼容入口；只能委托 canonical path，不得保存第二份事实。
- **ResidentHost**：常驻 Tauri 进程，可承载跨调用后台任务和事件派发器。
- **OneShot**：一次性 Engine 进程，单次调用完成后退出。

## 2. 背景与已确认故障

新架构已经落下以下骨架：

```text
AppRuntime                  进程级资源宿主
AppService                  Application workflow 边界
TaskRuntime                 任务生命周期内核
AgentExecutionRuntime       Agent 执行入口
AppError                    结构化错误
HostProcess/HostFilesystem  主机副作用边界
Extension Kernel            扩展共享基础设施
TargetCatalog               目标 Provider 描述目录
Domain Event/Outbox         已提交业务事实传播
```

当前主要问题不是缺少类型，而是新类型没有接管生产调用链：

| 领域 | 新 Authority | 仍存旧 Authority / 旁路 | 当前后果 |
|---|---|---|---|
| 错误 | `runtime::AppError` | `dto::AppResult = Result<T, String>` | 绝大多数 Application 错误仍不可分类 |
| 任务 | `TaskRuntime` | `BackgroundTaskRegistry` 内 8 组生命周期快照 | UI 投影与运行状态需双写 |
| 长任务 | `TaskRuntime` | scan、批量挂载同步 command | 无进度、取消、去重和关闭保护 |
| 进程 | `HostProcess` | conversation/Agent probe 自建 `Command` | 超时、输出上限、进程树清理语义不一致 |
| 扩展 | Extension Kernel | 领域私有 probe/launcher；kernel 只有 DTO | “共享内核”尚未拥有实际执行能力 |
| Agent | `AgentExecutionRuntime` | `legacy_gemini`、旧 CLI executor、旧配置键 | provider-neutral 路径仍双轨 |
| 目标 | AppRuntime `TargetCatalog` | 每次 `builtin()`、硬编码路径表 | 动态 provider 只在单元测试中成立 |
| 分层 | Runtime → lower layer | Runtime 反向调用 Application bootstrap | 依赖方向倒置 |

已确认的用户可见 P0 故障：AssetIWeave `0.6.1` 与 Agent Catalog 的
`>=0.5.0, <0.6.0` 不兼容，所有安装/更新请求在 preview 阶段被
`core_incompatible` 拒绝。生产入口还使用带 `fixture`、`example.com` 和占位
SHA256 的目录数据；这不是 ACP 握手失败，而是 Agent Market 发布链失效。

## 3. 总目标

本套规范完成后，必须满足：

```text
一个后端概念 → 一个 Authority → 多个无状态 Adapter/Projection
```

具体目标：

1. Application 层统一返回 `runtime::AppResult<T>`，transport 独立序列化错误。
2. 所有 ResidentHost 长任务由 `TaskRuntime` 决定生命周期。
3. source scan、批量 mount/unmount 成为可取消、可观察的后台任务。
4. 所有扩展启动和 probe 使用统一的 HostProcess 执行语义。
5. Extension Kernel 从“共享类型集合”升级为实际共享 launcher/probe 服务。
6. Agent Market 恢复安装/更新功能，并建立 Catalog 生产、缓存、发布闭环。
7. `TargetCatalog` 真正接管默认 Profile、目标识别和挂载规划。
8. Agent action assignment 使用唯一 schema；旧 Gemini/CLI 执行栈退出。
9. Runtime 不再依赖 Application；依赖守卫能够阻止回潮。

## 4. 非目标

- 不全面重写 Rust 后端。
- 不替换 SQLite、SQLx、Tauri、Go CLI 或 ACP SDK。
- 不把所有同步 Application API 一次性改成 async。
- 不引入跨进程持久化任务队列；OneShot Engine 仍可前台完成长操作。
- 不合并 Conversation Adapter manifest 与 Agent manifest。
- 不引入万能扩展 manifest 或运行时插件 ABI。
- 不改变 `asset_mounts` 作为挂载意图唯一事实源的产品决策。
- 不用删除缓存、放宽所有兼容检查等临时手段代替根因修复。
- 不把 fixture Catalog 当成生产目录继续扩展。

## 5. 技术栈与工程布局

| 层 | 技术/版本 | 主要目录 |
|---|---|---|
| Desktop UI | React 19、TypeScript、Vite、Tauri 2 | `frontend/src/` |
| Desktop/Engine backend | Rust stable，`rust-version = 1.96.0`、SQLx 0.9、Tokio 1 | `src-tauri/src/` |
| Agent protocol | `agent-client-protocol = 2.0.0`、stdio ACP、native CLI | `backend/agents/`、`backend/ai_execution/` |
| Persistence | SQLite、SQLx migrations | `src-tauri/migrations/`、`backend/store/` |
| CLI | Go 1.24、Cobra、Engine stdio JSON contract | `cli/` |
| Toolchain | Node 22、pnpm 10、rustfmt、Vitest | 根目录 scripts/config |

实现文件必须遵循现有目录所有权：workflow 放 `backend/application/`，跨 repository 的稳定
领域能力放 `backend/capabilities/`，SQL 放 `backend/store/`，传输适配放 `adapters/`，主机
副作用放 `host_*`，前端后端调用只放 `frontend/src/services/`。

## 6. 目标依赖方向

```text
React / Go CLI
      │
      ▼
Tauri Adapter / Engine Adapter
      │
      ▼
Application (AppService)
      │
      ├───────────────┬────────────────┐
      ▼               ▼                ▼
Capabilities     Domain Services     Runtime Services
      │               │                │
      └───────────────┴────────────────┘
                      │
                      ▼
       Store / HostProcess / HostFilesystem
                      │
                      ▼
              SQLite / OS / Network
```

强制规则：

- Runtime MUST NOT import Application。
- Store MUST NOT import Application、Tauri adapter 或执行主机副作用。
- Application MUST NOT 直接构造 `std::process::Command` 或
  `tokio::process::Command`。
- Frontend 页面、hook、组件 MUST 只经过 `frontend/src/services/` 调用后端。
- Tauri 和 Engine 对改变持久化状态的工作流 MUST 复用同一个 AppService 方法。

## 7. Canonical Authority 表

| 概念 | 唯一 Authority | 允许的 Projection/Adapter | 禁止的新事实源 |
|---|---|---|---|
| 请求上下文与资源 | `AppRuntime` | `AppService` clone、State wrapper | 每请求重开 DB/runtime |
| 应用错误 | `AppError` | `WireError`、日志 view | `Result<T, String>` Application API |
| 任务生命周期 | `TaskRuntime` | 领域 progress/result projection | adapter 自有 running/terminal 判定 |
| Agent 执行 | `AgentExecutionRuntime` | compatibility request mapper | vendor 专用 executor |
| 扩展进程 | `HostProcess` + Kernel launcher | domain invocation builder | domain 私有 command runner |
| 扩展生命周期冲突 | `LifecycleTaskCoordinator` | UI lifecycle snapshot | domain 私有 reservation map |
| Agent Catalog | `CatalogService` 选中的 active catalog | cache/bundled/remote candidate | UI 静态 runtime 元数据 |
| Target Provider | AppRuntime `TargetCatalog` | Profile DTO、路径展示 | `AppKind` 路径硬编码表 |
| Agent action assignment | canonical ActionId map | legacy settings migration | resolver 中永久 fallback |
| 挂载意图 | `asset_mounts` | status/observation projection | catalog 图标或 UI 本地状态 |

## 8. 文档目录

| 文件 | 主题 |
|---|---|
| `00-overview.md` | 总目标、边界、Authority 与执行规则 |
| `01-runtime-error-boundaries.md` | AppRuntime、AppError、分层与同步桥 |
| `02-task-runtime-workflows.md` | 任务 Projection、scan 与 batch mount |
| `03-process-extension-kernel.md` | HostProcess、probe、Extension Kernel |
| `04-agent-market-acp-recovery.md` | 当前 ACP/Market 故障、Catalog、缓存和发布闭环 |
| `05-target-catalog-capabilities.md` | TargetCatalog、ActionId、legacy Agent 收口 |
| `06-implementation-tasks.md` | 依赖排序后的可执行任务清单 |
| `07-verification-matrix.md` | 逐项验收证据和 CI 门禁 |
| `08-executor-runbook.md` | 面向后续代码模型的执行协议 |
| `09-preserved-invariants.md` | 已落地能力、事件脊柱、备份特例和不得回退项 |

## 9. 假设与既定裁决

1. 当前 `0.6.x` 是正在维护的 release line。
2. Catalog 若确认兼容整个 `0.6.x`，上界使用 `<0.7.0`；若协议不兼容，必须拆分
   catalog release，而不是跳过检查。
3. Tauri 是跨调用后台任务宿主；Engine OneShot 保持同步返回业务结果。
4. 旧 IPC 名称可以暂时保留，但必须委托 canonical workflow，并标注 deprecated。
5. 旧设置只在一次性迁移函数中读取；正常 resolver 不做永久 fallback。
6. Catalog 中展示“不兼容条目”是允许的，但 UI 必须明确禁用生命周期操作。
7. 当前 staged 文件 `agent-docs/feature-plans/后端里程碑审计建议.md` 是既有输入，执行者
   MUST NOT 擅自覆盖或重写。

## 10. 标准命令

```bash
pnpm check:boundaries
pnpm check:surface-matrix
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --lib
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:contract
pnpm cli:test:e2e
```

修改 Engine method、DTO、risk、confirmation 或 exposure 后 MUST 执行
`pnpm cli:contract`，不得手改 `cli/internal/schema/contract.json`。

## 11. 全局完成定义

只有 `07-verification-matrix.md` 中所有 `MUST` 项均有直接证据时，整个里程碑才可完成。
“测试通过”“类型已经存在”“没有搜索到更多问题”均不足以单独证明架构收口。
