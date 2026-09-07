# Google Antigravity 官方 ACP 接入契约

## 1. Problem Statement

AssetIWeave 当前把逻辑 Agent `antigravity` 绑定到系统命令 `agy` 和 `AgentProtocol::Native`。这使其绕过已经稳定存在的 ACP 执行链，并在 Native backend 内形成 Agent ID 分支、专属 stream-json 翻译和 `agy models` 发现逻辑。

Google 已在 ACP 官方 Registry 发布标准 ACP Server。继续保留旧绑定会让同一上游拥有两套执行身份，导致安装记录、连接状态、模型状态、Team 能力和 release evidence 相互矛盾。当前 ACP health 还把连接成功与非空模型列表绑定，无法正确表示“ACP 已连接但模型发现 unsupported/empty”。

## 2. Solution

保持 AssetIWeave 的逻辑 ID `antigravity` 不变，把其分发改为 Google 官方托管 Binary，并让动态 installation 生成 `AgentProtocol::Acp`。所有文本执行复用现有 `AcpExecutionBackend` 与 `AcpProtocol`。

同时把 runtime、protocol、model discovery 和 execution readiness 分成独立事实；旧 Native/System installation 按 catalog identity 失配标记为 incompatible，要求用户显式 reinstall。删除 AntiGravity 对 Native backend 的生产绑定，但保留通用 Native capability seam。

## 3. User Stories

1. 作为 AntiGravity 用户，我希望从 Agent Market 安装 Google 官方 ACP Server，以便使用官方标准协议。
2. 作为既有用户，我希望升级后仍使用逻辑 ID `antigravity`，以便能力分派不因上游 Registry ID 改名而丢失。
3. 作为既有 `agy` 用户，我希望应用明确提示 reinstall，而不是把 Direct-CLI 错当成 ACP Server。
4. 作为翻译用户，我希望 AntiGravity 自然通过通用 AgentExecutionRuntime 执行，以便与其他 ACP Agent 行为一致。
5. 作为 Memory 用户，我希望 AntiGravity 的文本任务复用同一 ACP backend，以便错误、取消和清理语义一致。
6. 作为设置用户，我希望连接成功表示 ACP handshake 成功，而不是模型列表恰好非空。
7. 作为设置用户，我希望模型发现失败单独展示，以便仍能区分运行时、协议和模型问题。
8. 作为默认模型用户，我希望模型列表 unavailable 时仍可执行不指定模型的文本任务。
9. 作为认证状态缺失的用户，我希望看到稳定的 auth-required 结果，以便登录后重试。
10. 作为发布维护者，我希望每个官方 archive 都有真实 SHA-256，以便不削弱 supply-chain 门禁。
11. 作为发布维护者，我希望未经真实 prompt smoke 的条目保持 experimental，以便 verification 含义可信。
12. 作为 Windows 用户，我希望 Market 选择官方 Windows executable 和正确架构。
13. 作为 Linux 用户，我希望启动参数包含 Registry 声明的 `--uid=`。
14. 作为 macOS Apple Silicon 用户，我希望安装官方 darwin arm64 archive 并执行 `.par` 入口。
15. 作为维护者，我希望接入另一个 ACP Agent 时无需复制 AntiGravity-specific backend。
16. 作为维护者，我希望 Native backend 继续服务真正的 Native Agent，而不是因本迁移被删除。
17. 作为 Team 用户，我希望未验证的 resume/history/live 能力被关闭，而不是继续隐式调用旧 `agy`。
18. 作为会话资产用户，我希望 Conversation Adapter 继续读取既有 AntiGravity 历史，不受执行协议迁移影响。
19. 作为资产装配用户，我希望 AntiGravity Target Profile 与 Skill 挂载路径保持不变。
20. 作为测试维护者，我希望空模型、认证失败、旧安装和 ACP 文本执行都由本地 fixture 稳定复现。
21. 作为安全审查者，我希望 ACP 失败后不执行 `agy` fallback，以免绕过新协议和清理规则。
22. 作为审查者，我希望最终搜索能证明生产代码没有 `antigravity -> NativeExecutionBackend` 路径。

## 4. 冻结决策

### C-01 身份

- 内部 ID 始终是 `antigravity`。
- `antigravity-acp` 只存在于 upstream metadata 与 release evidence。
- 不迁移 capability assignment 的 Agent ID。

### C-02 分发与完整性

- 只提供官方 Registry 已声明的平台 Binary distribution。
- `sha256` 继续是 64 位 lowercase hex 的必填字段。
- SHA-256 必须由实际下载的 archive 计算；禁止占位值、零值或旧版本 hash。
- archive 内 executable 必须经过 layout validation；不可把 `agy` 设为 executable。

### C-03 Protocol 与 Backend

- production definition 的 protocol 为 ACP。
- `AgentExecutor` 只按 definition protocol 选 backend；不按 Agent ID 选。
- `AcpProtocol` 不增加 AntiGravity 分支。
- Native backend 保留，但删除 AntiGravity selector、专属 model parser 与生产模块接线。

### C-04 状态语义

```text
installed          = installation row 存在
runtime_ready      = executable/layout/integrity 可用
protocol_connected = process + initialize + minimum session handshake 成功
model_ready        = 标准 ACP session config 提供可用模型目录
execution_ready    = enabled + installation ready + runtime ready + protocol connected
```

- `connected` 不读取 `model_status`。
- `execution_ready` 不读取 `model_status`。
- 动态 Registry candidate 不读取 `model_status`。
- 模型 discovery 的 `empty`、`unsupported` 或 `failed` 不覆盖 ready 的 protocol 状态。
- transport、spawn、initialize 或 session/new 失败才会把 protocol 标记为 failed/auth_required/unsupported。

### C-05 Connection probe 与 OneShot cleanup

- Connection probe 的成功判据是 initialize 与 session/new 成功。
- Connection probe 使用独立 probe outcome，不把 model parsing 放入连接路径。
- process reap 与 workspace removal 仍是强门禁。
- 标准 close/delete 按能力尽力执行，并记录 bounded cleanup outcome。
- 官方 Server 不声明 delete 时，探针可返回 `protocol_connected=true` 加 cleanup warning；该规则只属于 health probe，不改变业务 `AgentSessionMode::OneShot` 的强制删除契约。
- 若真实 smoke 证明 session/new 会持久化且无法通过标准能力或声明式命令删除，`AGACP-06` 必须标记阻塞，不能把条目标为 tested；另开 cleanup 规格，不在 ACP 层硬编码 Google 分支。

### C-06 Model discovery

- 只读取 `NewSessionResponse.configOptions` 中 `category=model` 或 `id=model` 的 select options，以及标准 available-models 兼容字段。
- 不调用 `agy models`，不新增 Vendor CLI fallback。
- 无模型目录时返回 model `unsupported` 或 `empty`；不得返回 protocol failure。
- Catalog 的 `modelDiscovery` 初始保持 false；只有 Google 1.1.1 真实 smoke 得到可用标准模型目录后才能改为 true。

### C-07 Authentication

- 本阶段只复用主机已有 Google/Antigravity credential。
- 认证错误映射为稳定的 auth-required 状态；installation 不标 broken。
- 不实现 OAuth UI、browser login、token persistence 或 credential manager。
- auth failure 不触发 Native fallback。

### C-08 Capability 降级

- 初始 catalog 只声明已验证的 `textPrompt` 与现有文本用途。
- `resume`、`historyReplay`、`liveEvents`、`richHistoryReplay`、`teamTools` 初始均为 false。
- 这会让已有 AntiGravity Team binding 在能力检查处失败；这是显式降级，不得用旧 Native path 补偿。
- 这些能力必须由独立 ACP/Team 规格与真实证据重新启用。

### C-09 旧 installation

- 当已安装 record 的 protocol、distribution type 或 distribution ID 与当前 catalog item 不兼容时，标记 `installation_status=incompatible`。
- 保留旧 record 和 `agy` 路径，供 UI 展示与显式 reinstall/uninstall；不得就地改写为 ACP。
- incompatible record 不进入动态 Registry，不具备 connected/execution_ready。
- catalog 列表应把它表示为需要 reinstall，而不是普通版本更新。

### C-10 Release evidence

- evidence 绑定当前 archive URL、distribution ID、size 与 SHA-256。
- conformance 分开记录 initialize、sessionNew、modelDiscovery、prompt、sessionUpdate、sessionClose、sessionDelete、cleanShutdown。
- 未运行项写 `not_run`，不支持项写 `unsupported`，失败项写 `failed`。
- verification 在完整真实 E2E 前为 `experimental`。

## 5. 非目标

- Conversation Adapter、其 package 或历史解析逻辑。
- Target Profile、Skill 挂载路径或 Asset Mount 语义。
- 完整 OAuth 或 Google account 管理。
- fs、terminal、MCP、permission UI 或 IDE coding workflow。
- 完整迁移 AntiGravity Team persistent session、history replay 或 live tool events。
- 删除 `AgentProtocol::Native` 或 `NativeExecutionBackend`。
- 修改 ACP SDK 或手写第二套 JSON-RPC client。
- 因展示版本变化引入 core/version 兼容门禁。

