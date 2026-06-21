# 仓库目录与架构边界

> 最后核对：2026-06-12

本文描述 AssetIWeave **当前代码实际采用的目录结构和职责边界**。它用于回答两个问题：

1. 新文件应该放在哪里？
2. 看到相似实现时，哪个是正式架构，哪个是兼容、生成或历史内容？

代码、测试和构建配置是当前实现的最终事实来源。`specs/` 记录产品需求和阶段设计，但可能包含尚未实现或已经演进的描述，不能仅凭旧 spec 新建第二套架构。

## 1. 运行时总览

```text
React frontend
  -> frontend/src/services
  -> Tauri commands
  -> src-tauri/src/service.rs
  -> scanner / planner / executor / conversations / store
  -> SQLx-managed SQLite and local filesystem

Go CLI
  -> Rust JSON-RPC engine
  -> command registry
  -> src-tauri/src/service.rs
  -> the same business and storage layers
```

核心约束：

- 前端和 Go CLI 都不能直接写 SQLite，也不能各自实现挂载规则。
- App 自有数据库 schema 只能通过 `src-tauri/migrations/` 中的 SQLx migration 演进。
- `AppService` 是桌面命令与 Engine 共用的应用服务入口。
- 文件扫描、挂载判断、计划和执行规则以 Rust 实现为准。
- Go CLI 负责命令体验、插件、策略接入和自更新，不复制 Rust 业务逻辑。
- `frontend/src/mock/` 只用于浏览器预览和后端不可用时的兜底，不是另一套数据源。

## 2. 仓库根目录

| 路径 | 状态 | 职责 | 不应放入 |
| --- | --- | --- | --- |
| `frontend/` | 正式 | React、TypeScript、Vite 前端 | SQLite、文件系统写入、挂载算法 |
| `src-tauri/` | 正式 | 完整 Rust 后端，包括 Tauri 壳、Engine、共享模型、存储和本地系统能力 | 前端展示状态、Go CLI 命令文案 |
| `cli/` | 正式 | Go CLI、Engine 客户端、命令树、插件、策略、自更新 | 直接写数据库或自行创建挂载链接 |
| `scripts/` | 正式辅助 | 构建、安装、契约生成和发布审计脚本 | 长期业务逻辑 |
| `docs/` | 正式文档 | 面向开发和使用的当前说明 | 里程碑勾选清单、工具生成的会话历史 |
| `specs/` | 需求与历史设计 | 产品需求、设计演进、任务里程碑 | 被当作当前代码目录地图 |
| `.specstory/` | 本地历史，已忽略 | 本地工具生成的会话记录 | 项目源码或需要维护的正式文档 |
| `dist/` | 生成，已忽略 | Vite 构建产物 | 手工编辑内容 |
| `target/`、`src-tauri/target/` | 生成，已忽略 | Rust 和本地 CLI 构建产物 | 源文件 |
| `node_modules/`、`frontend/node_modules/` | 生成，已忽略 | 包管理器依赖 | 项目代码 |

`Cargo.toml`、`package.json`、`pnpm-workspace.yaml` 是仓库级构建入口。不要在子目录重新建立一套重复的 workspace 或版本来源。

## 3. 前端目录

### 3.1 顶层职责

| 路径 | 职责 | 放置判断 |
| --- | --- | --- |
| `frontend/src/app/` | App 入口、全局 Provider、应用级能力 | 只放全局装配，不放页面业务 |
| `frontend/src/layouts/` | 长期存在的应用布局壳 | 侧栏、导航、页面外框 |
| `frontend/src/router/` | 内部导航模型、routeKey 和页面解析 | 当前不是 URL Router，不要另建 React Router 路由树 |
| `frontend/src/pages/` | 页面级编排 | 组合 hooks、领域组件和工具栏，不沉淀通用控件 |
| `frontend/src/components/` | 可复用 UI 与领域组件 | 按下面的组件层级放置 |
| `frontend/src/hooks/` | 页面/领域控制器与 React 状态编排 | 跨多个组件的操作流程 |
| `frontend/src/services/` | Tauri command 调用和边界适配 | 所有前端到 Rust 的调用入口 |
| `frontend/src/schemas/` | 运行时响应校验 | 校验跨进程数据，不承载 UI 状态 |
| `frontend/src/types/` | 前端共享 TypeScript 类型 | 不重复定义组件内部私有类型 |
| `frontend/src/store/` | 全局 Provider 状态 | 主题、设置等跨页面状态 |
| `frontend/src/theme/` | 主题 schema、token 和组件 recipe | 颜色、阴影、边框等视觉规则 |
| `frontend/src/styles/` | Tailwind 层和少量全局样式 | 不为单个组件建立全局选择器 |
| `frontend/src/i18n/` | 文案、导航翻译和本地化 Provider | 所有可见固定文案 |
| `frontend/src/manuals/` | routeKey 对应的应用内使用手册 | 不把手册内容塞进 `pages/` |
| `frontend/src/mock/` | 浏览器预览兜底数据 | 不参与 Tauri 正式运行时的数据写入 |
| `frontend/src/config/` | 静态资源配置 | 不放动态用户设置 |
| `frontend/src/utils/` | 无 React 状态的纯工具函数 | 不调用 Tauri、不维护全局状态 |
| `frontend/src/lib/` | 第三方或底层通用适配 | 不作为无法分类代码的收容目录 |

### 3.2 组件层级

`frontend/src/components/` 采用以下层级：

| 路径 | 层级 |
| --- | --- |
| `components/ui/` | 最底层通用控件，例如 Button、Input、Switch |
| `components/foundation/` | AssetIWeave 设计系统基础件，例如 Panel、DialogFrame、EmptyState |
| `components/common/` | 跨领域复合组件，例如确认框、数据工具栏 |
| `components/assets/`、`groups/`、`sources/` 等 | 领域组件，只服务对应业务概念 |

新增弹窗必须使用 `components/foundation/DialogFrame.tsx`。不要在领域目录重新实现 overlay、header、close button、滚动 body 或固定 footer。

组件落点顺序：

1. 只在一个页面使用：先放对应领域目录。
2. 被多个领域复用：提升到 `common/`。
3. 定义视觉或交互基础规范：放 `foundation/`。
4. 纯输入控件：放 `ui/`。

不要为了“以后可能复用”提前建立新的 primitives 层。

## 4. Rust 后端目录

### 4.1 单一后端包

仓库只维护一个 Rust package：`src-tauri/`。桌面程序和 stdio Engine 都通过
`assetiweave_lib` 复用同一套后端代码，不再拆分独立 core crate。

`src-tauri/src/models/` 是普通的后端共享模型模块，不代表 DDD 分层：

- `assets.rs`：Source、Asset、Profile、Mount、Deployment 等共享数据结构。
- `conversation.rs`：Conversation 标准化模型和纯辅助函数。
- `mod.rs`：仅负责模块导出。

模型、存储、服务和接口都留在 `src-tauri/src/` 内，按职责分模块即可。不要为了形式上的
“领域纯净”再建立 `domain`、`application`、`infrastructure` 或新的 workspace crate。

### 4.2 `src-tauri/src` 模块边界

| 路径 | 职责 |
| --- | --- |
| `main.rs` | 桌面二进制入口，不放业务逻辑 |
| `bin/assetiweave-engine.rs` | Engine 二进制入口，不放业务逻辑 |
| `models/` | 后端共享数据模型和无副作用辅助函数 |
| `lib.rs` | Tauri 插件、状态和 command 注册 |
| `commands.rs` | Tauri Controller：参数接收、锁、调用 `AppService` |
| `service.rs` | 桌面和 Engine 共用的应用服务门面与工作流编排 |
| `command_registry.rs` | Engine 方法、风险、schema 和 handler 的权威注册表 |
| `engine.rs`、`protocol.rs` | JSON-RPC stdio 和版本兼容协议 |
| `policy.rs`、`runtime.rs` | Engine 策略与调用生命周期 |
| `scanner/` | Source 遍历、分类、描述和 hash |
| `planner/` | 根据 mount/profile 生成可解释部署计划 |
| `targeting.rs` | 目标路径计算和真实挂载状态判断 |
| `executor/` | 执行计划、文件系统安全边界和部署状态写入 |
| `store/` | SQLx database 初始化、migration、SQL 常量、codec 和各领域 repository |
| `conversations/` | Conversation adapter、解析和标准化 |
| `defaults.rs` | 内置 Source、Profile、导航等默认值 |
| `path_utils.rs` | 路径展开、Git 路径和 hash 等共享工具 |
| `platform.rs` | 文件管理器等平台集成 |
| `app_settings.rs`、`logs.rs` | 独立基础能力 |
| `types.rs` | Tauri/App DTO、AppState 和非 core 类型 |

### 4.3 当前过渡热点

以下文件是正式入口，但已经是集中度较高的过渡热点：

- `src-tauri/src/service.rs`
- `src-tauri/src/commands.rs`
- `src-tauri/src/types.rs`
- `frontend/src/types/index.ts`
- `frontend/src/manuals/registry.ts`

它们不是 legacy，也不能另建 `service_v2.rs`、`commands_new.rs` 或第二套 `types/` 来绕开。新增代码应遵循：

- `service.rs` 只保留跨 repository/领域模块的工作流编排；
- 纯扫描、计划、执行、解析逻辑进入对应领域模块；
- SQL 进入 `store/` 对应 repository；
- 领域增长到难以浏览时，拆分现有模块并通过 `mod.rs` re-export；
- 拆分必须同步迁移调用方和测试，不保留永久双轨入口。

## 5. Go CLI 目录

| 路径 | 职责 |
| --- | --- |
| `cli/cmd/` | Cobra 命令、参数和用户交互 |
| `cli/errs/` | CLI 稳定错误分类 |
| `cli/extension/platform/` | 对外插件构建接口 |
| `cli/internal/client/` | Rust Engine 进程与协议客户端 |
| `cli/internal/schema/` | Engine 契约读取和生成的命令元数据 |
| `cli/internal/cmd*`、`hook/`、`protocol/` | 命令元数据、策略、hook 和协议辅助 |
| `cli/internal/platform/` | CLI 插件运行时和本地平台能力 |
| `cli/internal/update/`、`selfupdate/` | 更新检查与二进制替换 |
| `cli/internal/harvesters/`、`webharvester/` | CLI 管理的外部采集工作流 |
| `cli/tests/cli_e2e/` | CLI 到 Engine 的端到端验证 |

允许 Go CLI 自己实现的内容：

- Cobra 命令体验；
- CLI 插件、策略、输出格式和错误恢复；
- CLI 二进制更新；
- 外部采集器的安装与进程编排。

必须交给 Rust Engine 的内容：

- Source、Asset、Group、Profile 和 Mount 持久化；
- 扫描和资产身份判断；
- 挂载状态、计划和执行；
- 对 SQLite Catalog 的任何写入。

## 6. 生成文件与契约

`cli/internal/schema/contract.json` 是生成后提交的契约文件：

```text
src-tauri/src/command_registry.rs
  -> pnpm cli:contract
  -> cli/internal/schema/contract.json
  -> Go generated App commands
```

规则：

- 修改 Engine command、参数 DTO、风险或 exposure 后必须运行 `pnpm cli:contract`。
- 不手工修改 `contract.json` 来“修复”CLI。
- 生成文件必须能由单一命令重建。
- `dist/`、`target/` 和 `node_modules/` 不提交，也不作为代码审查依据。

## 7. `docs/`、`specs/` 与历史内容

三者用途不同：

| 类型 | 位置 | 作用 |
| --- | --- | --- |
| 当前开发/使用说明 | `docs/` | 描述现在如何工作、如何维护 |
| 产品需求和阶段设计 | `specs/` | 描述目标、验收标准和里程碑 |
| 本地工具历史 | `.specstory/` | 仅供本地追溯，已被 Git 忽略 |

当 spec 与代码不一致时：

1. 先确认是未完成目标、设计已变更，还是实现错误。
2. 不根据旧 spec 直接复制一套新模块。
3. 如果设计已变更，更新 spec 或明确标记旧段落。
4. 如果目标尚未实现，把它作为待办，不把文档描述成现状。

已知例子：旧 `specs/design.md` 曾描述 scanner/planner 位于共享 core；当前实际实现位于 `src-tauri/src/scanner` 和 `src-tauri/src/planner`。本文以当前代码边界为准。

## 8. 新文件落点决策

新增文件前按顺序判断：

1. **是否是用户界面？**
   - 页面编排放 `pages/`；
   - 领域 UI 放 `components/<domain>/`；
   - 全局布局放 `layouts/`；
   - 公共视觉基础件放 `components/foundation/`。
2. **是否调用 Tauri？**
   - 前端调用统一放 `services/`；
   - 不允许页面或 utils 直接新增 `invoke(...)`。
3. **是否是 Rust 业务规则？**
   - 纯共享模型/算法放 core；
   - 应用工作流放 `service.rs` 或拆出的领域 service；
   - App 自有 SQLite 读写放 `store/`，schema 变化放 `src-tauri/migrations/`；
   - 文件扫描/计划/执行放现有领域模块。
4. **是否是 CLI 特有体验？**
   - 放 `cli/`；
   - 若会改变业务数据，最终必须调用 Engine。
5. **是否是说明或设计？**
   - 当前维护说明放 `docs/`；
   - 需求和里程碑放 `specs/`；
   - 不提交工具会话历史。

如果一个文件无法按以上规则落点，先检查它是否混合了多个职责，而不是创建 `misc/`、`shared2/`、`legacy/` 或 `new/`。

## 9. 防止历史包袱的迁移规则

### 禁止的做法

- 新增 `FooV2`、`NewFoo`、`LegacyFoo` 后无限期并存。
- 在页面、CLI 或 mock 中复制 Rust 业务规则。
- 为一个领域建立第二套 service、types、schema 或 dialog 基础组件。
- 仅增加新入口，不迁移旧调用方。
- 把生成目录或本地历史加入 Git。

### 正确迁移方式

1. 明确旧入口、目标入口和所有调用方。
2. 先给目标行为补测试。
3. 迁移调用方。
4. 删除旧实现、旧导出和旧样式。
5. 搜索残留引用。
6. 运行对应验证。

确实需要兼容层时，必须满足：

- 名称明确表达 compatibility/adapter，而不是模糊的 `old`；
- 只有一个方向：旧入口委托给新入口；
- 不包含独立业务状态；
- 文档或 issue 中写明删除条件。

## 10. 变更检查表

提交结构性改动前检查：

- [ ] 没有新增第二套业务事实来源。
- [ ] 前端没有绕过 `services/` 调用后端。
- [ ] CLI 没有绕过 Engine 写数据或挂载文件。
- [ ] SQL 位于 `store/`，纯领域逻辑位于对应领域模块。
- [ ] 新弹窗和视觉基础件复用全局 foundation。
- [ ] 新类型没有与 core、Rust DTO 或现有前端类型重复。
- [ ] 旧入口、旧样式和旧测试已随迁移删除。
- [ ] 生成契约已更新。
- [ ] `docs/` 描述当前事实，`specs/` 描述目标和里程碑。
- [ ] 已运行受影响范围的类型检查、测试和构建。

## 11. 当前审计结论

截至 2026-06-12，仓库中没有发现被 Git 跟踪的 `old/`、`legacy/`、`v2/` 或完整重复前后端源码树。当前风险主要不是“遗留目录太多”，而是职责在少数大文件中再次混合：

1. `commands.rs` 与 `service.rs` 仍存在部分工作流集中和历史职责交叠。新能力应优先进入 `AppService` 与领域模块，不能继续把非 Controller 逻辑堆入 command。
2. `specs/design.md` 的部分目录描述已落后于实际代码。它应作为设计演进记录维护，不能覆盖当前目录事实。
3. Rust core、Tauri DTO 和 TypeScript 类型之间存在跨进程必要映射。新增字段应从 Rust 模型/DTO、command contract、前端 schema 到 TypeScript 使用方成链更新，不能只复制一个新接口。
4. `frontend/src/mock/` 与真实 Tauri service 并存是预览机制，不是双后端。正式运行时错误不能通过新增 mock 分支掩盖。
5. `.specstory/`、`.DS_Store`、`dist/`、`target/` 和 `node_modules/` 均属于本地历史或生成内容，已经通过 `.gitignore` 隔离，不构成正式架构。

后续架构治理应优先拆解职责热点和修正文档漂移，而不是新建平行目录。
