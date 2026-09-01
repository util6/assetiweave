# SPEC 文档集：Agent Marketplace 与动态运行时

> 将内置 ACP/Native Agent 重构为按需安装、可扩展、可动态加载的 Agent 市场

| 字段 | 值 |
|---|---|
| 状态 | Implemented；代码与自动化验收已冻结，历史人工评审记录保留 |
| 规格版本 | 1.0.0 |
| 日期 | 2026-08-16 |
| 代码基线 | AssetIWeave `bc5c14e` |
| 前置规格 | `SPEC_ ACP Agent Execution Runtime.md` 及其分册 |
| 产品名称 | Agent Market（不是仅限 ACP 的插件市场） |
| 首批协议 | ACP、Native |
| 首批分发类型 | System、Binary、Npx、Uvx |
| 首批迁移对象 | OpenCode、Gemini、Kiro、Antigravity、Claude、Codex、Hermes、Pi、Qoder |

## 1. 文档目标

本 SPEC 将当前“后端与前端各自硬编码全部 Agent、运行时临时解析命令”的架构，迁移为：

1. 用户从 Agent Market 查看可用 Agent。
2. 用户只安装需要的 Agent，不再把全部 Agent 当作已注册运行时。
3. 一个 Agent 可以声明多种分发方式，但只暴露一个逻辑 `agent_id`。
4. 安装器把远程分发物物化为本地、固定版本、执行时无需联网的运行时。
5. SQLite 记录当前租户的有效安装；动态 Registry 仅加载 `enabled` 且可执行的安装快照。
6. ACP/Native 执行核心继续复用现有实现，市场代码不得复制协议与进程管理器。
7. 新增标准 ACP Agent 时，通常只需增加/升级精选索引，不修改 Rust 或 TypeScript Vendor 分支。

本规格是实现约束，不是概念提案。执行模型必须按 `08-implementation-plan.md` 的单任务边界实施，不得一次性生成完整功能。

## 2. 文档清单与阅读顺序

| 顺序 | 文档 | 内容 | 主要读者 |
|---|---|---|---|
| 1 | [`01-product-requirements.md`](./agent-marketplace-dynamic-runtime/01-product-requirements.md) | 目标、范围、现状审计、需求、用户流程、非目标 | 产品、架构、全体执行者 |
| 2 | [`02-architecture-design.md`](./agent-marketplace-dynamic-runtime/02-architecture-design.md) | 分层、组件、依赖、动态 Registry、并发和 ADR | 架构、Rust 执行者 |
| 3 | [`03-catalog-and-distribution-contract.md`](./agent-marketplace-dynamic-runtime/03-catalog-and-distribution-contract.md) | 精选索引、System/Binary/Npx/Uvx 契约、选择算法、完整性 | 市场、安装器执行者 |
| 4 | [`04-installation-lifecycle-and-runtime-registry.md`](./agent-marketplace-dynamic-runtime/04-installation-lifecycle-and-runtime-registry.md) | 安装、更新、卸载、恢复、状态机、健康模型、动态重载 | 后端、任务系统执行者 |
| 5 | [`05-opencode-compatibility-migration.md`](./agent-marketplace-dynamic-runtime/05-opencode-compatibility-migration.md) | OpenCode System/managed 双兼容迁移及 CLI 兜底语义修正 | 迁移、执行 Runtime 执行者 |
| 6 | [`06-data-api-frontend-cli-integration.md`](./agent-marketplace-dynamic-runtime/06-data-api-frontend-cli-integration.md) | SQLite、DTO、Tauri、Engine、CLI、前端和设置迁移 | 全栈执行者 |
| 7 | [`07-security-testing-acceptance.md`](./agent-marketplace-dynamic-runtime/07-security-testing-acceptance.md) | 威胁边界、资源限制、测试矩阵、验收和质量门 | 安全、测试、评审者 |
| 8 | [`08-implementation-plan.md`](./agent-marketplace-dynamic-runtime/08-implementation-plan.md) | 按依赖排序的增量任务、文件白名单、验收和验证命令 | 任务编排者、执行模型 |
| 9 | [`09-execution-playbook.md`](./agent-marketplace-dynamic-runtime/09-execution-playbook.md) | Lunna/Flash 等代码模型的最小上下文和交付协议 | 模型调度者 |
| 10 | [`10-progress.md`](./agent-marketplace-dynamic-runtime/10-progress.md) | 实施状态、证据、偏差、阻塞和下一任务 | 所有参与者 |

## 3. 规范性语言

- **MUST / 必须**：不满足即视为实现不合格。
- **MUST NOT / 禁止**：实现中不得出现。
- **SHOULD / 应当**：默认采用；偏离必须记录理由和测试证据。
- **MAY / 可以**：可选行为，不得成为其他 MUST 的隐含前提。

冲突优先级：

1. 仓库根 `AGENTS.md` 与 Repository Guidelines。
2. 本索引的冻结决策与边界。
3. `01-product-requirements.md` 的 MUST / MUST NOT。
4. `02-architecture-design.md` 的模块边界和依赖方向。
5. `03`、`04`、`05` 的契约与状态不变量。
6. `06` 的跨层公开接口。
7. `07` 的验收和质量门。
8. `08` 的实施拆分。

前置 ACP SPEC 描述现有执行 Runtime；当它与本规格的市场、安装或动态 Registry 设计冲突时，以本规格为后续目标。ACP wire、进程清理和 Translation no-tool 约束仍沿用前置 SPEC。

## 4. 已确认的现状结论

### 4.1 并非所有 Agent ACP 都是 Node Package

当前内置九个 Agent 的执行或分发形态存在明确例外：

| Agent | 当前仓库启动方式 | 目标分发认识 |
|---|---|---|
| OpenCode | `opencode acp` | 官方 Registry 提供平台 Binary；同时可绑定已有 System CLI |
| Gemini | `gemini --acp` | 官方 Registry 可通过 Npx 分发 |
| Kiro | `kiro-cli-chat acp` | System CLI；当前命令与官方 `kiro-cli acp` 存在漂移，必须由精选索引修正 |
| Antigravity | `agy` | Native，不是 ACP；首版作为 System 分发进入统一 Agent Market |
| Claude | `npx -y @agentclientprotocol/claude-agent-acp@...` | Npx 分发 |
| Codex | `npx -y @agentclientprotocol/codex-acp@...` | Npx 分发；包内部可包装平台二进制，不能等同于“纯 Node 实现” |
| Hermes | `hermes acp` | Python/PyPI，可通过持久化 Uvx/uv tool 分发，也可绑定 System CLI |
| Pi | `npx -y pi-acp@...` | Npx 分发 |
| Qoder | `qodercli --acp` | 官方 Registry 提供固定 Npx 包；当前 Vendor 文档使用 System `qoder --acp`。两者应作为同一 item 的 Npx/System 候选，旧命令漂移由版本探测处理 |

因此，产品抽象必须是 `Agent -> distributions[] -> resolved runtime`，禁止抽象成 `Agent -> npm package`。

### 4.2 当前 `cli_fallback` 不是真正执行兜底

现有 `AgentDefinition.cli_fallback` 仅在连接检查失败时，把“CLI 版本探测成功”映射成 `connected=true`；实际 Translation 仍只走 `AgentExecutionRuntime -> ACP Backend`，不会回退到 `opencode run`。该布尔值混淆了安装、协议健康和执行路径，必须按 `05` 修正。

### 4.3 当前关键债务

- Rust `AgentRegistry::builtin()` 固定注册九个定义。
- 前端 `agentCatalog.ts` 再维护一份静态目录。
- `OnceLock` 使 Registry 无法在安装/卸载后动态重载，并隐含首个 DB path 绑定。
- Npx Agent 在执行命令中使用 `npx -y`，运行时可能联网并解析浮动外部状态。
- 设置页打开时并发探测全部 Agent，安装语义与连接语义混合。
- `AgentExecutor.active` 只保存取消令牌，无法判断某个 Agent 是否正在使用。
- 没有 Agent 安装的 SQLite 记录、后台安装任务和生命周期冲突控制。

## 5. 冻结架构决策

| ID | 决策 |
|---|---|
| D-101 | 产品边界命名为 **Agent Market**；ACP 是协议，不是唯一产品类型。 |
| D-102 | 市场项是声明式数据和分发说明，不允许加载包内任意 Rust/JS 插件代码到主进程。 |
| D-103 | 官方 ACP Registry 是上游数据源；客户端只消费 AssetIWeave 固定版本的精选索引及其缓存。 |
| D-104 | 一个逻辑 Agent 可以包含 System、Binary、Npx、Uvx 多种分发；用户安装后每租户仅有一个 active installation。 |
| D-105 | 执行时禁止联网安装、解析 Registry 或运行 `npx -y`/临时 `uvx`；安装器必须先物化固定版本本地入口。 |
| D-106 | 完整性采用分发生态的轻量证据：Binary SHA-256、npm lock/integrity、uv 固定版本；不递归哈希安装目录。 |
| D-107 | 首版不支持用户编辑 Agent 包、注册本地目录、Git 开发源、自定义命令或多版本回滚。 |
| D-108 | SQLite 只持久化当前安装；远程目录缓存使用原子 JSON + ETag，不建立目录发布历史表。 |
| D-109 | Registry 改为可原子替换的不可变快照；运行中的 execution 克隆定义，不受重载影响。 |
| D-110 | 市场生命周期复用后台任务、staging、原子激活、event + polling 模式，不复制 Conversation 插件的编辑/哈希/信任/版本历史设计。 |
| D-111 | OpenCode 是一个市场项、多种分发、唯一 ACP 执行路径；System CLI 探测不是 `opencode run` 执行兜底。 |
| D-112 | ACP 失败但 CLI 存在时：`installed=true`、`protocol_status=failed`、`execution_ready=false`、`connected=false`。 |
| D-113 | 安装、更新、卸载、刷新目录都是后台能力；不得持有全局 app lock 执行网络或大量文件 I/O。 |
| D-114 | 更新默认手动触发；新版本探测不得自动替换正在使用的版本。 |
| D-115 | capability assignment 继续保存在设置系统，但保存和执行时必须验证 Agent 已安装且 execution-ready。 |
| D-116 | 核心权限拒绝、MCP 注入、workspace 隔离、输出限制、进程清理和日志脱敏不可由市场包覆盖。 |

## 6. 首版完成定义

以下条件必须全部满足：

1. 全新安装时不要求九个 Agent 已存在；Market 仍可从 bundled/cache catalog 展示项目。
2. 安装一个 Agent 后，动态 Registry 只新增该 Agent，不注册其他未安装 Agent。
3. System、Binary、Npx、Uvx 四种分发均有离线 fixture 测试。
4. OpenCode 可选择绑定现有 CLI，或安装官方平台 Binary；二者都只执行 ACP。
5. Npx/Uvx 安装后执行命令只指向 app-owned 本地入口，执行阶段不产生包管理网络请求。
6. 安装/更新使用 staging 和原子激活；失败或取消不破坏旧版本。
7. 卸载 managed 安装会删除 app-owned 文件；卸载 System 绑定只解除记录，绝不删除外部 CLI。
8. 运行中的 Agent 会阻止其更新、卸载和删除；其他 Agent 和无关 UI 仍可操作。
9. `agent.catalog.list` 保持兼容；新增 Market/Installed/Task API 同时暴露给 Tauri 和 Engine，CLI contract 重新生成。
10. 前端移除静态目录作为真相源，并停止“打开设置即探测全部 Agent”。
11. capability picker 只允许选择 `enabled && installation_status=ready && execution_ready=true` 的 Agent。
12. 新增一个符合既有 ACP 能力的标准 Agent，只需修改精选索引 fixture/数据和测试，不增加 Vendor `if/match`。
13. 安全、恢复、响应性和跨层测试满足 `07-security-testing-acceptance.md`。

## 7. 非目标

首版明确不实施：

- 对话会话持久化、通用 Chat UI 或 Agent session 历史。
- 扩展 Translation 的 MCP、工具或 permission 能力。
- `opencode run` 或其他 CLI 的真正执行兜底。
- 自动更新、后台静默升级、更新频道、多版本切换和长期回滚历史。
- 用户自定义 Agent 包、本地目录、Git URL、任意命令、任意环境变量或包内配置编辑。
- 将远程 Agent 代码加载进 AssetIWeave 主进程。
- 启动或执行前递归计算目录哈希，或复制 Conversation 包的 trusted/changed/untrusted 模型。
- 由 AssetIWeave 下载和维护通用 Node、npm、Python、uv Runtime。Npx/Uvx 分发缺少宿主 Runtime 时必须显示前置依赖；Binary 不受此限制。
- 直接跟随官方 Registry 的 `latest` 自动安装未经精选验证的新版本。

## 8. 外部规范与证据

- ACP Registry Format：<https://github.com/agentclientprotocol/registry/blob/main/FORMAT.md>
- ACP Registry JSON Schema：<https://github.com/agentclientprotocol/registry/blob/main/agent.schema.json>
- ACP Registry CDN：<https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json>
- OpenCode 官方 Registry 项：<https://github.com/agentclientprotocol/registry/blob/main/opencode/agent.json>
- Kiro ACP 文档：<https://kiro.dev/docs/cli/acp/>
- Qoder ACP 文档：<https://docs.qoder.com/cli/acp>

外部 `latest` 只用于精选索引维护流程，不直接成为客户端执行输入。实施时若外部 Schema 与本规格快照不一致，必须停止对应目录同步任务并记录差异，不得静默放宽解析。

## 9. 文档维护规则

1. 决策变化先更新本索引和 `01/02`，再更新契约、任务和进度。
2. 目录或分发 Schema 变化必须同步更新 `03/06/07`。
3. 安装状态机变化必须同步更新 `04/06/07`。
4. OpenCode 行为变化必须同步更新 `05` 和前置 ACP SPEC 中 D-004/T23 相关结论。
5. Engine 方法或 DTO 变化必须运行 `pnpm cli:contract`，禁止手工编辑生成契约。
6. 每完成一个 Task，更新 `10-progress.md` 的证据、偏差和下一项。
7. 未经人工评审，不得把本规格状态改为 Approved；未经质量门，不得改为 Implemented。
