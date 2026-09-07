# 当前基线与外部事实

## 1. 审计快照

- 代码基线：`14e96d074c6ecbaf36b5b2c296339fbff1b5bc65`
- 分支：`refactor/ecosystem-task-1`
- Catalog：`assetiweave.agent-market/v1`，revision `2026.08.29.1`
- 审计日期：2026-09-07
- 工作区已有与本专项无关的未提交修改；Luna 每轮必须先运行 `git status --short`，不得覆盖或 reset。

## 2. 当前生产事实

| 区域 | 当前事实 | 目标差异 |
|---|---|---|
| Market catalog | `antigravity`, protocol native, system `agy`, tested | 改为 ACP managed Binary、experimental |
| Runtime Registry | production 由 installation 动态生成 | 保持，不重建静态 production registry |
| 测试 builtin Registry | AntiGravity 被写成 Native/`agy` | 更新测试 fixture/expectation |
| Executor | 按 `AgentDefinition.protocol` 路由 | 保持该高层路由 |
| Native backend | 按 Agent ID 选择专属 Antigravity runner | 删除该 production selector |
| Native model discovery | 解析 `agy models` TSV | AntiGravity 不再使用 |
| ACP connection | `run_connection_probe` 调用 session probe 后强制解析模型 | 改为 handshake 与模型解耦 |
| ACP model discovery | 从 session config 解析 | 复用，不加 Vendor fallback |
| Installation readiness | connected/execution_ready 依赖 ACP model_status=ready | 删除模型依赖 |
| Registry candidate SQL | ACP row 需要 model_status=ready | 删除模型依赖 |
| ACP health refresh | 只跑 discover_models，并用结果同时写 protocol/model | 拆为 connection 与 model 两阶段 |
| Frontend legacy metadata | `antigravity = Native Agent` | 改为 `ACP Agent` |
| Release evidence | Native/System/agy evidence | 改为官方 Binary ACP evidence |
| Team capability | resume/history/live 已声明且 Native 专属代码已存在 | 本阶段关闭，禁止 fallback |

## 3. 已确认代码接缝

### 3.1 最高测试接缝

首选 `AgentRuntimeManager` + 临时 SQLite + 本地 fake ACP executable：一次测试可观察 installation health、动态 Registry publication、协议路由与 model status。不要为每个私有 helper 建立重复测试。

### 3.2 辅助接缝

- Catalog/release：现有 Node release gate。
- 协议行为：现有 `fake-acp-agent.mjs` 与 ACP backend tests。
- 路由：现有 `AgentExecutor` fake backend tests。
- 安装失配：Agent installation repository/runtime startup recovery integration test。
- UI metadata：`agentCatalog` 与 Agent Settings 组件测试。
- 真机：官方 Binary 的 stdio smoke，不作为确定性 CI 依赖。

## 4. 上游 Registry 事实

截至 2026-09-07，官方仓库 `agentclientprotocol/registry` 中：

```text
Registry ID: antigravity-acp
Name: Google Antigravity
Version: 1.1.1
Publisher: Google LLC
License: proprietary
Update commit: 81bf71b55e15f630c4fb8a86d20d3088071d2071
Update time: 2026-09-03T16:34:24Z
```

官方 metadata：

| Distribution ID | Target | Archive | Executable | Launch args | HTTP size observed 2026-09-07 |
|---|---|---|---|---|---:|
| `binary-darwin-aarch64` | darwin/aarch64 | `https://dl.google.com/agy-extensions/releases/macos/agy-acp-server-agy_acp_server_1.1.1-darwin-arm64.zip` | `agy_acp_server.par` | `[]` | 316,014,828 |
| `binary-linux-x86_64` | linux/x86_64 | `https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-agy_acp_server_1.1.1-linux-x86_64.zip` | `agy_acp_server.par` | `["--uid="]` | 681,969,407 |
| `binary-linux-aarch64` | linux/aarch64 | `https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-agy_acp_server_1.1.1-linux-arm64.zip` | `agy_acp_server.par` | `["--uid="]` | 656,572,786 |
| `binary-windows-x86_64` | windows/x86_64 | `https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-agy_acp_server_1.1.1-windows-x86_64.zip` | `agy_acp_server.exe` | `[]` | 468,238,392 |
| `binary-windows-aarch64` | windows/aarch64 | `https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-agy_acp_server_1.1.1-windows-arm64.zip` | `agy_acp_server.exe` | `[]` | 468,521,191 |

Registry metadata未提供 SHA-256。本基线没有下载约 2.6 GB 的全部制品，因此五个 SHA-256 当前状态为 `MISSING_EVIDENCE`。Nixpkgs 中 1.0.0/RC01 的 hash 只证明旧制品，禁止复用于 1.1.1。

## 5. 已观察 initialize 能力

原始调查对 1.0.0/RC01 观察到：

```text
protocolVersion = 1
loadSession = true
prompt image/audio/embeddedContext = true
session list/resume = supported
```

ACP Registry 1.1.1 protocol matrix仍报告 `loadSession`、`session/list`、`session/resume`。这些是上游协议证据，不等于 AssetIWeave 已完成 Team E2E，也不证明 model config、prompt、close/delete 或 auth 已通过。

## 6. 当前冲突与施工含义

1. `team-chat-workspace` 的文档和生产代码把 AntiGravity 定义为 Direct-CLI Provider。新目标明确取代该执行绑定；本阶段应关闭 Team 能力，不应并行保留两条 production route。
2. Issue #18 要求真实 OneShot session 删除，而官方 initialize 证据未声明 delete。Connection probe 必须与业务 OneShot cleanup 分开建模；真实 prompt smoke 未证明删除前，verification 不能提升。
3. 当前 `model_status` 是字符串并支持 `unchecked/ready/failed/unsupported`；无需 migration 即可表达目标。不要新增数据库列，除非代码审计证明公开状态无法表达，届时停止报告。
4. 当前 `InstallationStatus::Incompatible` 已存在，但 startup recovery 未按活动 catalog identity 对比；优先复用该状态，不新增 legacy 表。

## 7. 不可触碰清单

- `builtin-assets/adapters/antigravity/`
- `builtin-assets/targets/antigravity.json`
- Conversation 数据库、Adapter DTO 与内容投影
- fs/terminal/MCP capability
- 用户已有未提交文件

