# 交接记录：AGACP-06

## 基本信息

- Task ID：AGACP-06
- Commit：`2145df5`
- 执行日期：2026-09-07
- 基线 HEAD：`230cec7`
- 工作区修改：
  - `builtin-assets/agent-market/catalog-v1.json`
  - `builtin-assets/agent-market/release-evidence-v1.json`
  - `scripts/check-agent-catalog-release.test.mjs`
  - `agent-docs/feature-plans/antigravity-official-acp/07-progress.md`

## 1. 修改文件列表

| 文件 | 变更类型 | 说明 |
|---|---|---|
| `builtin-assets/agent-market/catalog-v1.json` | 修改 | 将 `antigravity.capabilities.modelDiscovery` 从 `false` 更新为 `true`（经实测 1.1.1 在 `session/new` 返回 11 个真实 Gemini 模型配置） |
| `builtin-assets/agent-market/release-evidence-v1.json` | 修改 | 同步更新 `catalogContentSha256`；为 `binary-darwin-aarch64` 记录真实 partial conformance 状态及 `realSmoke` 实测结构体（包含 initialize, session/new, model discovery, prompt 回复, exit 验证）；其他 4 平台严格保持 experimental / not_run，绝不伪造 |
| `scripts/check-agent-catalog-release.test.mjs` | 修改 | 将测试套件中的 `modelDiscovery` 期望值更新为 `true` |
| `agent-docs/feature-plans/antigravity-official-acp/07-progress.md` | 修改 | 标记 AGACP-06 为 PASS，记录官方 1.1.1 协议事实与决策日志 |

## 2. 官方 Binary 1.1.1 真实 Smoke 验证证据

- **测试环境**：Darwin aarch64 (macOS 15.3.1, Apple Silicon), Node.js v22.14.0
- **制品下载与校验**：
  - URL: `https://edgedl.me.gvt1.com/edgedl/gemini-code-assist/antigravity-agent/1.1.1/darwin-arm64/agy_acp_server_1.1.1-darwin-arm64.zip`
  - 下载文件大小: 316,014,828 字节
  - SHA-256 验签: `fdfa915652cdb7ba8085cc8fffed072cbe009251aa2c951aabdda07a8c28a189`（与 catalog/evidence 严格一致）
  - 解压产物: `agy_acp_server.par` (802,163,856 字节) 与伴生 harness `localharness_external` (80,248,304 字节)
  - 权限保证: 赋予两文件可执行权限（`0o755`）
- **协议握手与交互实测（无文件系统/终端/MCP，脱敏验证）**：
  1. `initialize`:
     - Protocol Version: 1
     - Agent Info: `name: "antigravity-acp"`, `title: "Google Antigravity"`, `version: "agy_acp_server_1.1.1"`
     - Capabilities: `loadSession: true`, `promptCapabilities: { image: true, audio: true, embeddedContext: true }`, `mcpCapabilities: { http: true, sse: true }`, `sessionCapabilities: { list: true, resume: true }`, `auth: { logout: true }`
  2. `session/new`:
     - 成功建立 session，返回 11 个模型选项（包括 `gemini-3.7-flash-high` [默认], `gemini-3.8-flash`, `gemini-3.1-pro` 等）
     - 证实官方 binary 具备真实模型发现与选择能力
  3. `session/prompt`:
     - Prompt 输入: `"Reply with exactly: ASSETIWEAVE_ACP_OK"`
     - 收到流式更新 chunk，内容完全匹配 `"ASSETIWEAVE_ACP_OK"`
     - 正常完结: `stopReason: "end_turn"`
  4. 进程回收与生命周期:
     - 优雅关闭 stdin，主进程及其拉起的底层 `localharness_external` 干净退出（Exit code: 0）
     - 确认无孤儿进程遗留，无临时文件残留
- **关键架构发现与验证**：
  - 官方 1.1.1 的 `sessionCapabilities` 仅包含 `list` 与 `resume`，**未声明 `close` 与 `delete`**。
  - 这证明我们在 AGACP-05 中实现的 unsupported cleanup 保护（无法调用 session/delete 时优雅关闭 stdin/SIGTERM 回收进程并清理临时工作区）是完全契合上游实际情况的必须设计。
  - 依照门禁规则，未完全实现全套协议能力的 agent 在 release evidence 中必须记录 `acpConformance.status = "partial"`，item verification 保持 `"experimental"`。

## 3. 门禁验证结果

| 命令 | PASS/FAIL | 摘要 |
|---|---|---|
| `node --test scripts/check-agent-catalog-release.test.mjs` | PASS | 12 个测试全数通过 |
| `node scripts/check-agent-catalog-release.mjs --release` | PASS | Catalog 与 Release Evidence 校验全数通过 |
| `CARGO_TARGET_DIR=target/test-target cargo test -p assetiweave --lib -- backend::agent_market` | PASS | 全部 agent_market 单元与端到端测试通过 |

## 4. 契约核对

- [x] logical ID 仍为 `antigravity`
- [x] 无 AntiGravity-specific ACP backend/protocol 分支
- [x] 无 `agy` fallback
- [x] model 状态未污染 protocol/readiness
- [x] 旧 Native installation 不进入 Registry
- [x] Binary SHA-256 来自真实 archive
- [x] Conversation Adapter 无改动
- [x] Target Profile 无改动
- [x] Native 通用 seam 保留
- [x] 未验证能力没有写 passed（仅 Darwin aarch64 实测标记 partial，其他 4 平台严格保持 experimental / not_run）

## 5. 下一任务

- AGACP-07（最终代码审计与全流程门禁）
