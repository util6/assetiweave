# AntiGravity 官方 ACP 接入改造执行文档

## 0. 文档目的

将 AssetIWeave 当前的 AntiGravity Agent 执行方式，从项目内现有的 `Native` 特殊实现迁移为 Google 官方发布的标准 ACP Server。

本任务的核心目标不是重构整个 Agent Runtime，也不是扩展完整 IDE 能力，而是：

> **让 AssetIWeave 将 AntiGravity 视为一个标准 ACP Agent，并通过现有 `AcpExecutionBackend` / `AcpProtocol` 完成连接、模型发现和文本执行。**

执行过程中应尽量复用现有 ACP 架构，不允许为 AntiGravity 新增 Vendor-specific ACP Backend。

---

# 1. 已确认的外部事实

截至 2026-09-02，ACP 官方 Registry 已公开 Google 官方 AntiGravity ACP Server。

上游 Registry Agent：

```text
Registry ID: antigravity-acp
Name: Google Antigravity
Version: 1.0.0
Publisher: Google LLC
Protocol: ACP v1
```

官方二进制来自 Google 域名：

```text
https://dl.google.com/agy-extensions/...
```

当前官方发行包括：

```text
macOS aarch64
Linux x86_64
Linux aarch64
Windows x86_64
Windows aarch64
```

执行入口：

```text
macOS/Linux:
agy_acp_server.par

Windows:
agy_acp_server.exe
```

Linux Registry 当前额外声明：

```text
--uid=
```

已实测 `initialize` 可以正常完成，返回：

```json
{
  "protocolVersion": 1,
  "agentCapabilities": {
    "loadSession": true,
    "promptCapabilities": {
      "image": true,
      "audio": true,
      "embeddedContext": true
    },
    "sessionCapabilities": {
      "list": {},
      "resume": {}
    }
  },
  "agentInfo": {
    "name": "antigravity-acp",
    "title": "Google Antigravity",
    "version": "agy_acp_server_20260818_01_RC01"
  }
}
```

因此本任务可以假设：

```text
AntiGravity 官方 ACP Server 是真实存在且可以进行 ACP v1 initialize 的。
```

但不得假设所有 ACP 能力已经验证完成。

特别是以下能力仍需通过实际运行确认：

```text
session/new
session/prompt
session/update
model config options
authentication
MCP
fs
terminal
```

---

# 2. 当前 AssetIWeave 状态

当前项目已经拥有标准 ACP 执行链：

```text
AgentExecutor
    ↓
AcpExecutionBackend
    ↓
AcpProtocol
    ↓
agent_client_protocol Rust SDK
    ↓
stdio JSON-RPC
```

当前 ACP 层已经实现：

```text
initialize
session/new
session/set_config_option
session/prompt
session/cancel
session/close
session/delete
session/update normalization
```

因此：

> AntiGravity 不应该拥有单独的 ACP Backend 或 Protocol 实现。

---

## 2.1 当前 AntiGravity 特殊实现

当前 Agent Registry / Market 中 AntiGravity 被定义为：

```text
logical id: antigravity
protocol: native
command: agy
```

大致执行链：

```text
AssetIWeave
    ↓
AgentExecutor
    ↓
NativeExecutionBackend
    ↓
agy
```

这套绑定已经不再符合上游现状。

---

# 3. 目标架构

改造后：

```text
AssetIWeave
    ↓
Capability Assignment
    ↓
AgentExecutionRuntime
    ↓
AgentExecutor
    ↓
AgentProtocol::Acp
    ↓
AcpExecutionBackend
    ↓
AcpProtocol
    ↓
ACP v1 / stdio JSON-RPC
    ↓
agy_acp_server.par / agy_acp_server.exe
    ↓
Google Antigravity
```

必须满足：

```text
AntiGravity == 普通 ACP Agent
```

不得出现：

```rust
if agent_id == "antigravity" {
    // special ACP implementation
}
```

除非是纯 distribution metadata 或经过明确证明不可避免的平台参数差异。

---

# 4. Agent Identity 规则

AssetIWeave 内部逻辑 ID 继续保持：

```text
antigravity
```

不要改成：

```text
antigravity-acp
```

`antigravity-acp` 只作为上游 Registry ID。

目标关系：

```text
AssetIWeave Agent ID:
antigravity

ACP Registry ID:
antigravity-acp

Google runtime binary:
agy_acp_server
```

原因：

AssetIWeave 已存在以下稳定 Domain Identity：

```text
builtin-assets/adapters/antigravity
builtin-assets/targets/antigravity.json
Agent assignments
Agent installation records
frontend metadata
conversation adapters
```

本任务禁止因为执行协议变化破坏这些逻辑 ID。

---

# 5. 必须修改的范围

## 5.1 Agent Market Catalog

重点文件：

```text
builtin-assets/agent-market/catalog-v1.json
```

当前 AntiGravity：

```text
protocol = native
system distribution = agy
```

目标：

```text
protocol = acp
managed binary distributions
```

AntiGravity Market Item 应调整为：

```text
id: antigravity
displayName: Google Antigravity
protocol: acp
upstream.registryId: antigravity-acp
```

描述应明确：

```text
Google official Antigravity ACP server
```

不要继续描述成：

```text
native Agent runtime
```

---

## 5.2 Distribution

为官方支持平台配置 Binary Distribution。

建议：

```text
binary-darwin-aarch64
binary-linux-x86_64
binary-linux-aarch64
binary-windows-x86_64
binary-windows-aarch64
```

其中：

### macOS

```text
archive:
Google 官方 darwin-arm64 zip

executable:
agy_acp_server.par

launchArgs:
[]
```

### Linux

```text
archive:
对应 Linux 官方 zip

executable:
agy_acp_server.par

launchArgs:
["--uid="]
```

必须以 ACP Registry 当前真实 metadata 为准。

### Windows

```text
archive:
对应 Windows 官方 zip

executable:
agy_acp_server.exe

launchArgs:
[]
```

---

# 6. SHA-256 规则

AssetIWeave 当前 Binary Distribution 强制：

```rust
sha256: String
```

并且要求合法 64 位 lowercase hex。

不要为了 AntiGravity 修改为：

```rust
Option<String>
```

也不要放松 Binary integrity 校验。

Google ACP Registry 当前未提供 SHA-256 时，应采用：

```text
Google official archive
        ↓
AssetIWeave release/curation process
        ↓
计算 archive SHA-256
        ↓
写入 curated catalog
```

因此：

> 保持 AssetIWeave 现有 supply-chain 安全模型不变。

如果当前执行环境无法可靠计算真实 archive hash：

```text
禁止填 fake hash
禁止使用 0000...
禁止关闭校验
```

可以先完成代码和 catalog structure，但必须明确留下 blocking TODO，不得伪造可安装状态。

---

# 7. Agent Registry 兼容定义

检查：

```text
src-tauri/src/backend/agents/registry.rs
```

当前测试/legacy builtin 中大致存在：

```rust
builtin_agent(
    "antigravity",
    "Antigravity",
    AgentProtocol::Native,
    "agy",
    [],
    "agy",
)
```

目标：

AntiGravity 不再被测试为 `Native`。

如果该 builtin registry 已仅用于测试：

```text
只修测试和 legacy expectation。
```

不要重新把 managed binary path 硬编码到 builtin registry。

真实 production runtime 必须继续来自：

```text
Agent Market
→ AgentInstallation
→ definition_from_installation()
→ dynamic AgentRegistry
```

不得重新建立第二套静态 production registry。

---

# 8. Frontend Compatibility Metadata

检查：

```text
frontend/src/components/settings/agentCatalog.ts
```

当前 legacy fallback 类似：

```text
["antigravity", "Native Agent"]
```

改为：

```text
["antigravity", "ACP Agent"]
```

这里只允许修改展示 / compatibility metadata。

不要在 frontend 写：

```text
binary URL
command path
launchArgs
package version
```

这些仍然必须由 Agent Market DTO 提供。

---

# 9. 不应该修改的模块

以下模块原则上不属于本次执行协议迁移。

不要修改：

```text
builtin-assets/adapters/antigravity/
```

原因：

这是 Conversation Conversion Adapter。

它解决的是：

```text
如何读取 / 解析 AntiGravity 历史资产
```

不是：

```text
如何执行 AntiGravity Agent
```

---

不要修改：

```text
builtin-assets/targets/antigravity.json
```

原因：

Target 定义的是 Asset Deployment Target，例如：

```text
~/.antigravity/skills
```

它与 Agent Runtime Protocol 无关。

---

不要因为 AntiGravity 不再使用 Native 而删除：

```text
AgentProtocol::Native
NativeExecutionBackend
```

本任务只删除：

```text
AntiGravity → Native
```

这一绑定。

Native 作为未来 Runtime Capability seam 继续保留。

---

# 10. ACP Protocol 层原则

原则上：

```text
src-tauri/src/backend/agents/protocol/acp.rs
```

不应该因为 AntiGravity 而增加 Vendor-specific 逻辑。

如果当前：

```text
initialize
session/new
prompt
session/update
```

能够与 AntiGravity 官方 Server 正常交互，则：

```text
不要修改 AcpProtocol。
```

只有实际协议测试证明 Google 官方 ACP 与标准 SDK 存在兼容差异时，才允许修改。

即使修改，也必须针对：

```text
ACP protocol compatibility
```

而不是：

```text
if antigravity
```

---

# 11. 必须检查并修复：ACP connection 与 model discovery 耦合

这是本任务中需要特别审视的现有问题。

当前：

```rust
run_connection_probe()
```

实际类似：

```text
initialize
↓
session/new
↓
parse_session_models()
↓
有非空 model list
↓
connection success
```

这意味着：

```text
ACP Connected
```

错误地依赖：

```text
Model Discovery Ready
```

另外：

```rust
AgentInstallation::connected()
```

当前也可能依赖：

```text
protocol_status == Ready
&&
model_status == ready
```

这是两个不同的状态维度，不应耦合。

---

## 11.1 目标状态语义

至少区分：

```text
installed
runtime_ready
protocol_connected
model_discovery_ready
execution_ready
```

ACP connection 应由：

```text
process spawn
+
initialize
+
minimum usable ACP session probe
```

决定。

不应由：

```text
model list 是否非空
```

决定。

---

## 11.2 Connection probe

目标：

```text
initialize OK
session/new OK
```

应该足以证明：

```text
ACP protocol connection ready
```

如果 cleanup 需要：

```text
session/close
session/delete
```

则继续正常执行 cleanup。

但是：

```text
model list empty
```

不能把 connection 改成 failed。

---

## 11.3 Model discovery

Model discovery 应单独执行：

```text
initialize
↓
session/new
↓
read configOptions / availableModels
```

可能得到：

```text
ready
failed
unsupported
empty
```

这些状态不得反向修改：

```text
protocol connection == failed
```

除非错误实际发生在：

```text
initialize
session/new
transport
```

等 protocol 层。

---

# 12. AntiGravity Model Discovery

不要继续基于：

```text
agy models
```

来实现 AntiGravity 模型发现。

官方 ACP Server 应优先使用标准 ACP session response。

当前：

```text
AcpExecutionBackend::discover_models()
```

已经支持从：

```text
NewSessionResponse.configOptions
```

寻找：

```text
category == model
或
id == model
```

并解析：

```text
options
currentValue
```

因此：

> AntiGravity model discovery 首先走标准 ACP session config options。

如果实测 AntiGravity 返回标准 model config：

```text
直接复用现有代码。
```

如果不返回：

```text
记录 model discovery unsupported / unavailable。
```

不要立刻重新增加：

```text
agy models
```

Vendor CLI fallback。

---

# 13. Authentication 范围

Google 官方 ACP Server 可能涉及：

```text
OAuth
Google login
Gemini API key
existing local credentials
```

当前 AssetIWeave ACP Client 并没有完整 ACP Authentication workflow。

因此本任务只要求：

### Phase 1

支持：

```text
使用当前主机已有登录态 / credential
启动官方 agy_acp_server
完成 ACP 标准执行
```

如果已有认证态时：

```text
initialize
session/new
prompt
```

能够成功，本阶段即满足目标。

---

## 13.1 本任务不要求

不要在本次任务中实现完整：

```text
OAuth UI
browser login
ACP authenticate method
token persistence
credential manager
Google account management
```

如果无登录态返回 auth error：

必须：

```text
正确失败
返回可理解 error
不 crash
不 fallback 到旧 native agy execution
```

Authentication 作为独立后续 SPEC。

---

# 14. fs / terminal / MCP 不属于本次 Scope

当前 AssetIWeave 是：

```text
Text AI Execution Runtime
```

不是完整 IDE ACP Client。

当前 initialize client capabilities 对：

```text
terminal
fs
```

保持关闭是允许的。

本任务不需要：

```text
让 AntiGravity 修改项目文件
执行 shell
运行 IDE coding workflow
提供完整 MCP runtime
```

如果 Agent 在 Translation 等当前 Purpose 中主动要求：

```text
tool call
permission
fs
terminal
```

继续遵守现有 fail-closed policy。

不得为了“让 AntiGravity 看起来更完整”扩大本任务范围。

---

# 15. Translation 目标

现有：

```text
Translation
→ AgentExecutionRuntime
→ AgentExecutor
```

如果 capability assignment 指向：

```text
antigravity
```

则最终应自然 route：

```text
AgentProtocol::Acp
→ AcpExecutionBackend
```

不得新增：

```rust
if antigravity {
    special_translation(...)
}
```

不得建立：

```text
AntiGravityACPBackend
```

不得使用：

```text
NativeExecutionBackend fallback
```

---

# 16. Migration / Existing Installation

需要检查当前用户已经安装的：

```text
antigravity
protocol = native
distribution = system-antigravity
program = agy
```

旧 installation record。

如果 catalog 更新后旧 installation 仍存在：

不得把：

```text
agy
```

错误解释成新的 ACP server。

推荐策略：

```text
旧 Native AntiGravity installation
→ incompatible / needs reinstall
```

用户重新安装：

```text
Google official ACP managed binary
```

不要静默：

```text
agy → ACP
```

因为二者根本不是同一个 executable。

不要因为 logical ID 相同就自动认定旧 installation 可继续使用。

---

# 17. Release Evidence

更新：

```text
builtin-assets/agent-market/release-evidence-v1.json
```

旧 AntiGravity evidence 类似：

```text
assetiweave-native-antigravity-...
nativeConformance
```

新的 evidence 应明确是：

```text
ACP official binary
```

至少记录：

```text
catalogItemId
upstreamAgentId = antigravity-acp
agentVersion
distributionId
distributionType = binary
sourceAgentUrl / registry evidence
artifact URL
sha256
```

ACP conformance 至少区分：

```text
initialize
sessionNew
modelDiscovery
prompt
sessionClose
cleanShutdown
```

如果某项尚未验证：

```text
partial / not_run
```

禁止写成：

```text
passed
```

---

# 18. Verification Status

第一版建议：

```text
experimental
```

除非真实 E2E 已覆盖：

```text
install
hash verify
spawn
initialize
session/new
prompt
response streaming
cleanup
```

之后才考虑：

```text
tested
```

不要因为 Google 官方发布就自动标记：

```text
tested
```

AssetIWeave 的 `verification.status` 表示：

```text
AssetIWeave 自己验证过
```

而不是：

```text
上游是官方
```

---

# 19. 测试要求

至少补齐以下测试。

## AG-ACP-01

Catalog：

```text
antigravity.protocol == acp
```

---

## AG-ACP-02

AntiGravity 不再使用：

```text
AgentProtocol::Native
```

---

## AG-ACP-03

每个平台 binary distribution：

```text
target 正确
archive 正确
executable 正确
launchArgs 正确
sha256 合法
```

---

## AG-ACP-04

Legacy frontend：

```text
Antigravity == ACP Agent
```

---

## AG-ACP-05

Dynamic installation 生成：

```text
AgentDefinition.protocol == Acp
```

---

## AG-ACP-06

连接检查：

```text
initialize success
session/new success
models empty
```

必须得到：

```text
connected = true
```

而不是 connection failed。

---

## AG-ACP-07

模型发现失败：

```text
model_status = failed / unsupported
```

不得导致：

```text
protocol_status = failed
```

前提是 ACP handshake/session 本身正常。

---

## AG-ACP-08

Translation：

```text
agent_id = antigravity
```

必须 route：

```text
AcpExecutionBackend
```

不得调用：

```text
NativeExecutionBackend
agy
```

---

## AG-ACP-09

Auth unavailable：

```text
ACP returns authentication failure
```

应该：

```text
stable error
cleanup process
cleanup workspace
no native fallback
```

---

## AG-ACP-10

旧 Native installation：

不得被误认为新 ACP installation 已 ready。

---

# 20. Real Smoke Test

如果当前环境允许运行 Google 官方 binary，至少执行：

```text
1. install / materialize
2. spawn
3. initialize
4. session/new
5. inspect returned session config
6. discover model list
7. prompt simple text
8. receive session/update
9. receive PromptResponse
10. close / cleanup
```

测试 prompt 使用无副作用文本，例如：

```text
Reply with exactly: ASSETIWEAVE_ACP_OK
```

验收：

```text
response contains expected text
process cleanly reaped
workspace removed
no orphan process
```

不要用修改文件 / terminal / tool call 作为本阶段 smoke test。

---

# 21. 明确禁止事项

执行 Agent 不得：

```text
1. 新建 AntiGravity-specific ACP Backend
2. 在 AcpProtocol 中加入 agent_id == antigravity 分支
3. 删除 NativeExecutionBackend
4. 修改 conversation adapter
5. 修改 target deployment semantics
6. 放松 SHA-256 校验
7. 使用 fake SHA-256
8. 把 agy 当作官方 ACP executable
9. 在 ACP 失败后 fallback 到 agy
10. 静默迁移旧 Native installation 为 ACP
11. 为本任务实现完整 OAuth
12. 为本任务开启 fs / terminal / MCP
13. 因 models 为空而判定 ACP connection failed
14. 声称未经测试的 ACP capability 已通过验证
```

---

# 22. 推荐执行顺序

严格建议按以下顺序实施。

### Step 1

确认当前 upstream Registry：

```text
antigravity-acp
```

的最新官方 distribution metadata。

不要凭本文中的 URL 做永久假设。

---

### Step 2

审计当前：

```text
antigravity
NativeExecutionBackend
AgentProtocol::Native
```

所有 production 引用。

区分：

```text
真正 production binding
vs
test / legacy fallback
```

---

### Step 3

更新 Agent Market catalog。

完成：

```text
native → acp
system agy → official managed binary
```

---

### Step 4

处理真实 SHA-256。

禁止绕过 integrity。

---

### Step 5

修改 legacy/test expectations。

---

### Step 6

修复：

```text
ACP connected
≠
model discovery ready
```

状态耦合。

---

### Step 7

运行现有 ACP tests。

保证：

```text
OpenCode
Codex
Gemini
etc.
```

没有回归。

---

### Step 8

新增 AntiGravity ACP tests。

---

### Step 9

如果环境允许，运行 Google official binary smoke test。

---

### Step 10

更新 release evidence。

只记录真实完成的验证。

---

# 23. 最终验收条件

任务完成必须同时满足：

- [ ] AssetIWeave logical Agent ID 仍为 `antigravity`
- [ ] upstream Registry ID 为 `antigravity-acp`
- [ ] AntiGravity protocol 为 `acp`
- [ ] 不再通过 `agy` 执行 Agent
- [ ] 使用 Google 官方 ACP Server binary
- [ ] Binary integrity 仍强制 SHA-256
- [ ] AntiGravity 复用现有 `AcpExecutionBackend`
- [ ] `AcpProtocol` 无 Vendor-specific 分支
- [ ] Translation 可以 route 到 AntiGravity ACP
- [ ] ACP connection 状态与 model discovery 状态解耦
- [ ] model discovery 走 ACP session config
- [ ] auth failure 可以安全失败
- [ ] 没有 Native fallback
- [ ] 旧 Native installation 不会被误识别为 ACP ready
- [ ] Conversation Adapter 无无关改动
- [ ] Target deployment 无无关改动
- [ ] Native capability seam 继续保留
- [ ] 现有 ACP Agent tests 无回归
- [ ] AntiGravity 新增针对性测试
- [ ] release evidence 不伪造测试结论

---

# 24. Codex / Luna 执行要求

在开始修改前：

```text
先审计实际代码，不要仅按本文猜测当前实现。
```

如果本文与当前仓库存在差异：

```text
以当前代码的领域边界为准，
但必须保持本文定义的架构目标与禁止事项。
```

执行时优先：

```text
最小改动
复用现有 abstraction
删除 AntiGravity 特例
不扩大 scope
```

完成后必须输出：

```text
1. 修改文件列表
2. 每个文件修改目的
3. 实际执行的测试
4. 测试结果
5. 未完成/无法验证的事项
6. 是否完成真实 AntiGravity ACP E2E
7. 是否存在剩余 Native AntiGravity production path
```

最后再次执行代码搜索，确认生产代码中不存在：

```text
AntiGravity → NativeExecutionBackend
```

的残留执行链。