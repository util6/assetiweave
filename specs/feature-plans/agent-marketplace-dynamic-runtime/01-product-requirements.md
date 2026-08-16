# Product Requirements：Agent Market

| 字段 | 值 |
|---|---|
| 状态 | Proposed |
| 规格版本 | 1.0.0 |
| 目标阶段 | MVP |
| 前置能力 | 已实现 ACP/Native Agent Execution Runtime |

## 1. 问题陈述

当前产品把“支持哪些 Agent”和“本机安装了哪些 Agent”当作同一件事：后端硬编码所有 Agent，前端复制目录，运行时再探测命令。这导致：

- 用户不需要的 Agent 也出现在运行 Registry 和设置选择中。
- Npx 命令在执行期可能临时下载，安装状态不可观察、不可恢复。
- 新 Agent 需要同时修改 Rust、TypeScript 和测试，扩展成本随应用数量增长。
- Agent 是 Node 包、平台二进制、Python 工具或系统 CLI 的差异没有被建模。
- OpenCode 的 CLI 版本探测被误标为执行连接成功。
- 安装、协议健康、可执行性和 capability assignment 互相混淆。

产品需要一个按需、声明式、可审计且保持本地优先的 Agent Market。

## 2. 用户与场景

### 2.1 主要用户

1. **普通用户**：只安装用于翻译、Memory 等具体能力的一个或少数 Agent。
2. **高级用户**：本机已安装 OpenCode/Kiro/Qoder，希望 AssetIWeave 复用现有 CLI，而不是重复下载。
3. **维护者**：从官方 ACP Registry 选择并验证固定版本，发布小型精选索引。
4. **自动化调用者**：通过 Go CLI/Engine 查看、安装、更新、检查和卸载 Agent。

### 2.2 核心用户旅程

#### Journey A：按需安装 managed Agent

1. 用户打开 `设置 -> Agent -> 市场`。
2. 客户端使用缓存或 bundled catalog 立即渲染，再按需刷新精选索引。
3. 用户查看 Agent 的协议、版本、分发、下载大小、Runtime 前置条件和验证状态。
4. 用户点击安装，看到确定的分发方式、路径、版本和资源预算。
5. 后台任务下载/安装、验证、激活并动态重载 Registry。
6. Agent 出现在 Installed 和 capability picker 中；未安装 Agent 不出现于 picker。

#### Journey B：绑定现有 OpenCode CLI

1. Market 检测到兼容的 `opencode` System CLI。
2. 安装预览提供“使用现有安装”和“安装受管 Binary”两个明确选择。
3. 用户选择 System；安装器只记录解析后的入口和版本，不复制或哈希 CLI。
4. ACP conformance 成功后 `execution_ready=true`。
5. ACP conformance 失败但版本探测成功时仍显示已绑定/已安装，但 `connected=false` 且不能用于 capability。

#### Journey C：手动更新

1. Catalog 显示已验证的新固定版本。
2. 用户明确触发更新。
3. 新版本安装到 staging 并完成验证，旧版本继续服务。
4. 激活事务和 Registry swap 成功后才删除旧 managed 目录。
5. 任一步失败时旧版本保持 active。

#### Journey D：卸载

1. 系统检查该 Agent 是否有活跃 execution、生命周期任务或 capability assignment。
2. 有活跃 execution 时拒绝卸载并返回 `agent_in_use`。
3. 有 assignment 时要求用户确认清除哪些引用；不静默改用其他 Agent。
4. managed 安装删除 app-owned 目录；System 安装只删除绑定记录。
5. Registry 原子移除该定义。

## 3. 产品范围

### 3.1 MVP 包含

- Agent Market 目录浏览、缓存刷新、详情和安装预览。
- Installed 列表、健康检查、启用/停用、更新、重装、卸载。
- System、Binary、Npx、Uvx 分发。
- ACP 和现有 Native 协议标识。
- 后台安装任务及进度、取消、event + polling。
- 动态 Registry 和执行冲突控制。
- capability assignment 校验及旧设置迁移。
- Desktop/Tauri、Engine 和 Go CLI 的同等业务能力。

### 3.2 MVP 不包含

非目标以主索引第 7 节为准。任何“顺便支持自定义包、自动更新、回滚历史、插件编辑、Node Runtime 下载”的实现都属于越界。

## 4. 术语模型

| 术语 | 定义 |
|---|---|
| Agent | 用户可选择的逻辑 AI 执行提供者，以稳定 `agent_id` 标识。 |
| Protocol | AssetIWeave 与 Agent 进程交互的协议；MVP 为 `acp`、`native`。 |
| Market Item | 精选目录中的声明式 Agent 元数据和分发列表。 |
| Distribution | 获取/绑定 Agent Runtime 的一种方式：System/Binary/Npx/Uvx。 |
| Installation | 某租户当前绑定到某 Agent 的唯一有效分发和本地入口。 |
| Managed | 文件位于 app-owned 目录，由 AssetIWeave 安装和删除。 |
| System | 复用用户环境已有命令；AssetIWeave 只绑定，不拥有文件。 |
| Installation Status | 安装物是否存在、完整、兼容的持久状态。 |
| Protocol Status | 最近一次 ACP/Native 健康检查结果。 |
| Execution Ready | 是否满足当前执行的全部必要条件；不是 `installed` 的同义词。 |
| Curated Index | AssetIWeave 发布的固定版本、经过兼容性筛选的 Agent 目录。 |
| Upstream Registry | ACP 官方 Registry；用于维护精选索引，不由客户端直接信任执行。 |

## 5. 功能需求

### 5.1 Market 和目录

| ID | 需求 |
|---|---|
| FR-CAT-001 | 客户端 MUST 在无网络时使用 bundled catalog 展示首批 Agent。 |
| FR-CAT-002 | 刷新 MUST 请求 AssetIWeave 精选索引，使用 ETag/If-None-Match，并原子替换缓存。 |
| FR-CAT-003 | 客户端 MUST 验证 catalog schema、版本、唯一 ID、固定版本和分发字段；无效缓存不得覆盖最后有效版本。 |
| FR-CAT-004 | Catalog MUST 区分 `protocol` 与 `distribution.type`。 |
| FR-CAT-005 | 一个 item MUST 可声明多个 distribution，且不得把同一 Agent 拆成“System 版”和“managed 版”两个 Agent。 |
| FR-CAT-006 | 每个精选版本 MUST 包含 core compatibility、测试标识、用途能力和来源元数据。 |
| FR-CAT-007 | Market list MUST 合并当前安装摘要，但不得用远程目录覆盖本地安装真相。 |
| FR-CAT-008 | 新标准 ACP Agent 的目录接入 SHOULD 为纯数据变更；Vendor-specific 逻辑必须通过架构评审。 |

### 5.2 安装预览和分发选择

| ID | 需求 |
|---|---|
| FR-SEL-001 | 安装前 MUST 解析平台、架构、宿主 Runtime、可用 System 命令、版本范围和资源限制。 |
| FR-SEL-002 | 有多个可用分发时 MUST 展示选择，不得静默改变 ownership。 |
| FR-SEL-003 | 默认排序 MUST 为：兼容 System、managed Binary、Npx、Uvx；catalog 可禁用某选项，但不得把不兼容项选为默认。 |
| FR-SEL-004 | 预览 MUST 显示 Agent、精确版本、distribution、ownership、目标路径、下载大小或 unknown、外部依赖和权限影响。 |
| FR-SEL-005 | Npx 缺少兼容 Node/npm、Uvx 缺少 uv 时 MUST 标记 `runtime_missing`，不得在后台静默安装 Runtime。 |
| FR-SEL-006 | 用户提交的 install request MUST 引用 catalog item/version/distribution ID；前端不得提交任意 program、args 或 env。 |

### 5.3 安装、更新、重装和卸载

| ID | 需求 |
|---|---|
| FR-LIFE-001 | Desktop/Tauri 生命周期写操作 MUST 作为后台任务快速返回 snapshot；一次调用即启动一次 Engine 进程的 CLI 使用同步 `*.run` wrapper 等待同一 lifecycle workflow 终态。 |
| FR-LIFE-002 | 每个租户、每个 Agent 同时最多一个 lifecycle task；冲突返回现有任务或 `installation_conflict`。 |
| FR-LIFE-003 | managed 安装 MUST 使用 task staging 目录；验证通过前不得写 active installation。 |
| FR-LIFE-004 | Binary MUST 校验精选索引给出的 SHA-256；Npx/Uvx MUST 使用精确版本和包管理器完整性元数据。 |
| FR-LIFE-005 | 激活 MUST 先提交 SQLite 当前安装，再原子发布 Registry snapshot；失败时必须恢复一致状态。 |
| FR-LIFE-006 | 更新失败 MUST 保留旧 installation、旧目录和旧 Registry definition。 |
| FR-LIFE-007 | System 卸载 MUST 只解除绑定；禁止删除、移动或修改外部 program。 |
| FR-LIFE-008 | managed 卸载 MUST 仅删除记录所指向、且位于 agent runtime root 下的目录。 |
| FR-LIFE-009 | 重装 MUST 保持相同 version/distribution，除非请求明确指定另一 catalog 版本。 |
| FR-LIFE-010 | 取消 MUST 收敛子进程、清理 staging、保留旧 active installation，并产生终态事件。 |
| FR-LIFE-011 | 应用启动 MUST 清理过期 staging 并修复可判定的“DB 已激活但 Registry 未加载”内存状态。 |
| FR-LIFE-012 | 应用退出 MUST 对活跃安装任务给出与其他后台任务一致的中断提示。 |

### 5.4 执行 Runtime 与动态 Registry

| ID | 需求 |
|---|---|
| FR-RUN-001 | Registry MUST 从当前租户的 installation rows 构建不可变快照。 |
| FR-RUN-002 | install/update/enable/disable/uninstall 后 MUST 触发原子重载，无需重启应用或 Engine。 |
| FR-RUN-003 | execution 开始时 MUST 克隆 resolved definition；后续 Registry swap 不得改变该 execution。 |
| FR-RUN-004 | `AgentExecutor.active` MUST 记录 execution ID、agent ID、installation identity 和 cancellation。 |
| FR-RUN-005 | 更新或卸载 MUST 在 active execution 存在时返回 `agent_in_use`，不得先取消用户任务。 |
| FR-RUN-006 | 执行阶段 MUST 只启动安装时解析的绝对入口或已绑定的 System 入口，不运行 package manager 安装命令。 |
| FR-RUN-007 | 执行阶段 MUST NOT 获取 catalog、下载 artifact、运行 `npx -y` 或临时 `uvx`。 |
| FR-RUN-008 | 市场不得改变现有 ACP initialize/session/new/prompt/cancel/cleanup 和 Translation no-tool 策略。 |
| FR-RUN-009 | Agent 未安装或未 ready 时，执行 MUST 返回稳定错误，不得静默切换 assignment 或 Agent。 |

### 5.5 安装、协议健康和 capability

| ID | 需求 |
|---|---|
| FR-HLT-001 | API MUST 分别返回 `installed`、`installation_status`、`runtime_status`、`protocol_status`、`execution_ready`。 |
| FR-HLT-002 | `installed` 仅表示存在安装/绑定记录和预期入口，不代表 ACP 已连接。 |
| FR-HLT-003 | OpenCode CLI 版本探测成功但 ACP 失败时 MUST 返回 `connected=false`、`execution_ready=false`。 |
| FR-HLT-004 | 安装后 conformance MUST 至少覆盖 program start、ACP initialize、session/new 和 clean shutdown；Native 使用对应最小健康契约。 |
| FR-HLT-005 | model discovery 是可选能力；失败不得使支持默认模型的 Agent 安装失效，但必须显示独立错误。 |
| FR-HLT-006 | capability picker MUST 只展示 enabled 且 execution-ready 的安装。 |
| FR-HLT-007 | 保存 capability assignment 和每次执行 MUST 再验证 readiness，避免陈旧 UI 状态。 |
| FR-HLT-008 | 页面打开 SHOULD 使用持久化/缓存健康摘要；连接检查按 item 懒加载或由用户显式触发，不得 probe-all。 |

### 5.6 用户界面

| ID | 需求 |
|---|---|
| FR-UX-001 | Agent 设置 MUST 至少提供 `市场` 和 `已安装` 两个视图。 |
| FR-UX-002 | Market card MUST 展示协议、精选版本、可用分发、平台适配、测试状态和外部依赖。 |
| FR-UX-003 | Installed row MUST 展示 ownership、版本、安装状态、协议状态、模型、更新时间和可用动作。 |
| FR-UX-004 | 任务 MUST 在发起位置显示进度；用户离开页面后全局任务区域仍可观察。 |
| FR-UX-005 | 运行中仅禁用同一 Agent 的冲突动作；过滤、导航和其他 Agent 操作保持可用。 |
| FR-UX-006 | assignment 指向未安装 Agent 时 MUST 显示原 Agent 和安装 CTA，不得静默改为 OpenCode/Gemini。 |
| FR-UX-007 | 首版 MUST 移除或隐藏伪造的 `Add Custom` 入口。 |
| FR-UX-008 | 错误显示 MUST 使用稳定 code 对应的用户消息，并保留可重试/重装/安装 Runtime 等明确动作。 |

### 5.7 Desktop、Engine 与 CLI

| ID | 需求 |
|---|---|
| FR-API-001 | 改变持久状态的业务逻辑 MUST 位于 AppService/backend，不得只存在于前端或 Go CLI。 |
| FR-API-002 | Tauri 和 Engine MUST 共享同一 DTO、校验、store 和 lifecycle service；Tauri 采用 start/get/cancel，当前 one-shot Engine 采用同步 run，传输生命周期不同但业务语义相同。 |
| FR-API-003 | Engine contract 变化 MUST 通过 `pnpm cli:contract` 生成，禁止手工编辑。 |
| FR-API-004 | Go CLI MUST 通过 Engine 调用，不得写 SQLite 或直接删除 runtime 目录。 |
| FR-API-005 | 现有 `agent.catalog.list`、connection、models 接口在迁移期 MUST 保持可调用，并使用兼容映射。 |

## 6. 非功能需求

| ID | 需求 |
|---|---|
| NFR-001 | Market 使用缓存/bundled 数据的首屏响应目标 ≤ 200 ms，不等待网络。 |
| NFR-002 | start lifecycle command 返回初始 snapshot 目标 ≤ 250 ms，不包含下载时间。 |
| NFR-003 | Registry snapshot lookup 不执行 SQLite、网络或文件遍历。 |
| NFR-004 | 安装期间不得持有 `AppState.lock` 执行阻塞 I/O。 |
| NFR-005 | 进度事件丢失后，polling 必须在下一周期恢复最终状态。 |
| NFR-006 | 日志不得包含认证值、完整环境变量值、prompt/result、包管理 token 或远程原始 stderr。 |
| NFR-007 | catalog、DB、Registry 三者不一致时执行以已提交 DB + 最近成功 Registry snapshot 为界，不得执行 staging 内容。 |
| NFR-008 | 所有路径删除必须证明目标位于 app-owned runtime root；System 路径永不删除。 |

## 7. 成功指标

### 7.1 行为指标

- 新安装默认 Registry 条目数为 0；只随用户安装增加。
- 标准 ACP Agent 数据接入不增加 Vendor 分支。
- 所有 managed execution 的 program 都解析到 app-owned 目录，且参数不含 `-y`。
- 设置页一次打开不会为所有 Market item 创建进程。
- 更新/取消失败注入测试中旧版本可继续执行。

### 7.2 维护性指标

- 后端只有 catalog 解析器和通用 distribution installer 理解分发类型。
- 前端不维护 Agent ID 到 command/package 的映射。
- Conversation package manager 的 hash/trust/edit/history 类型不进入 Agent Market 模块。
- 每个实施 Task 修改不超过 5 个手写文件；生成契约文件作为明确例外记录。

## 8. 产品验收场景

| ID | Given | When | Then |
|---|---|---|---|
| AC-P01 | 全新数据库且本机无 Agent | 打开 Market | bundled catalog 可见，Installed 为空，capability picker 无候选 |
| AC-P02 | Market 有 Claude Npx 固定版本且 Node 满足 | 用户安装 | 后台完成后本地入口存在、Registry 有 Claude、执行不触发下载 |
| AC-P03 | 本机有兼容 OpenCode | 用户选择 System | 记录 system ownership，ACP 成功后可选为 capability |
| AC-P04 | OpenCode `--version` 成功但 `acp` handshake 失败 | 检查连接 | installed=true、connected=false、execution_ready=false |
| AC-P05 | Agent A 正在执行 | 用户卸载 A | 返回 agent_in_use；Agent B 操作和导航不受影响 |
| AC-P06 | Agent 有旧版本 | 新版本校验失败 | 任务失败，旧版本和 Registry definition 保持不变 |
| AC-P07 | System Agent 已绑定 | 用户卸载 | DB/Registry 移除，外部 executable 未改变 |
| AC-P08 | assignment 指向未安装 Agent | 发起 Translation | 返回 agent_not_installed，并保留 assignment |
| AC-P09 | catalog cache 损坏且网络离线 | 启动应用 | 忽略损坏缓存，使用 bundled catalog |
| AC-P10 | 新增标准 ACP item fixture | 只更新精选索引 | Market、安装、Registry 和 ACP execution 全链路通过，无 Vendor 代码 |

## 9. 待人工评审但已有默认值的产品参数

若评审未提出相反结论，实施使用以下默认值：

- 更新策略：仅手动。
- 每租户每 Agent active version：1。
- Market catalog：官方上游 + AssetIWeave 精选发布 + bundled fallback。
- managed root：`~/.assetiweave/agent-runtimes`，允许现有 app home 解析逻辑覆盖实际根。
- Npx/Uvx 宿主 Runtime：用户/系统提供，应用只探测和提示。
- 安装最大总时长：10 分钟。
- Binary 下载上限：512 MiB；解压后上限：1 GiB；文件数上限：20,000。
