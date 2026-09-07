# Google Antigravity 官方 ACP 接入执行路由

| 字段 | 值 |
|---|---|
| 专项目标 | 将逻辑 Agent `antigravity` 从 Native Direct-CLI 迁移到 Google 官方 ACP Server |
| 执行模型 | Luna；每轮只执行一个 `AGACP-*` 工作包 |
| GitHub Issue | `#26` |
| 基线提交 | `14e96d074c6e`（2026-09-06） |
| 上游快照 | ACP Registry `antigravity-acp` 1.1.1，提交 `81bf71b55e15f630c4fb8a86d20d3088071d2071` |
| 初始发布级别 | `experimental` |

## 1. Luna 必读顺序

1. `AGENTS.md`
2. `CONTEXT.md`
3. `agent-docs/adr/0011-on-demand-acp-agent-marketplace.md`
4. `agent-docs/adr/0012-extension-kernel-and-runtime-refactor.md`
5. 本目录 `01-contract.md`
6. 本目录 `02-current-baseline.md`
7. 本目录 `03-work-packages.md`
8. 本目录 `04-verification-matrix.md`
9. 本目录 `05-luna-execution-playbook.md`
10. 每轮结束填写 `06-handoff-template.md` 与 `07-progress.md`

`docs/knowledge/AntiGravity 官方 ACP 接入改造执行文档.md` 是原始对话与调查上下文，不是按当前代码校准后的执行清单。冲突时优先级为：当前代码与测试 > 已接受 ADR > 本专项契约 > 原始知识文档。

## 2. 唯一目标态

```text
Capability Assignment
  -> AgentExecutionRuntime
  -> AgentExecutor
  -> AgentDefinition.protocol = Acp
  -> AcpExecutionBackend
  -> AcpProtocol
  -> Google agy_acp_server
```

身份固定为：

```text
AssetIWeave logical Agent ID = antigravity
ACP Registry ID              = antigravity-acp
runtime executable           = agy_acp_server.par / agy_acp_server.exe
```

不得新增 `AntiGravityAcpBackend`、ACP Vendor 分支或 `agy` fallback。

## 3. 执行顺序与门禁

```text
AGACP-00 上游制品固化与 SHA-256
  -> AGACP-01 ACP connection / model discovery / readiness 解耦
  -> AGACP-02 旧 Native installation 失配治理
  -> AGACP-03 Catalog、release evidence 与前端兼容元数据
  -> AGACP-04 移除 AntiGravity Native production route
  -> AGACP-05 Fake ACP 端到端回归
  -> AGACP-06 Google 官方二进制真实 smoke
  -> AGACP-07 全量门禁与最终审计
```

硬门禁：

- `AGACP-00` 未取得五个平台真实 SHA-256 前，不得提交声称可安装的五平台 Binary catalog。
- `AGACP-01` 未证明 model list 为空时 protocol 仍为 ready，不得更新 AntiGravity catalog。
- `AGACP-02` 未证明旧 `agy` installation 不进入动态 Registry，不得切换 catalog identity。
- `AGACP-04` 未完成前，不得宣称不存在 Native AntiGravity production path。
- `AGACP-06` 未完成真实 prompt smoke 时，verification 保持 `experimental`；不得写 `tested`。
- 任一工作包发现需要修改 Conversation Adapter、Target Profile 或启用 fs/terminal/MCP，立即停止。

## 4. 与现有专项的关系

- 本专项遵守 ADR-0011 的按需 Agent Market 和受控进程模型。
- 本专项遵守 ADR-0012 的动态 Registry、Extension Kernel 与 AppService 边界。
- 本专项覆盖旧 Agent Market 文档中“Antigravity 是 Native/System”的历史决策；旧文档只作为历史材料，不再指导新施工。
- `team-chat-workspace` 已实现的 Antigravity Direct-CLI Session 与本目标态冲突。本专项不把它迁移成完整 IDE/Team ACP 能力；AntiGravity 的 `resume/historyReplay/liveEvents/teamTools` 在获得真实 ACP 证据前必须关闭。现有 Team 绑定应表现为能力不满足，而不是退回 `agy`。
- Issue #18 的 OneShot 删除契约继续有效。连接探针不是业务执行成功；若官方 Server 不声明 delete，必须记录 cleanup 结果，并按 `01-contract.md` 的探针语义处理，不能放松真实 OneShot 执行的删除要求。

## 5. 完成定义

- `antigravity` 的生产 Agent Definition 只能由 ACP Binary installation 生成。
- 旧 `system-antigravity` / `agy` installation 为 incompatible，不能进入运行时 Registry。
- ACP 连接成功只依赖 process、initialize 与最低 session handshake，不依赖模型列表。
- model discovery 具有独立状态，失败或 unsupported 不把 protocol 状态改为 failed。
- execution readiness 不依赖非空模型列表；默认模型可执行时仍可执行文本任务。
- Translation/Memory 通过通用 ACP backend 路由，无 Vendor 分支与 Native fallback。
- Native backend 仍作为通用 capability seam 存在，但不再包含 AntiGravity production selector。
- Conversation Adapter 与 Target Profile 没有改动。
- release evidence 只记录实际执行过的验证。
- 所有自动化门禁通过；真实 E2E 状态在交接中明确为 PASS、FAIL 或 NOT_RUN。
