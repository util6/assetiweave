# 工作包与依赖图

## AGACP-00：固化官方制品证据

**目标**：为 Registry 1.1.1 的五个 archive 取得可复核 SHA-256 与 archive layout。

**允许文件**：

- `agent-docs/feature-plans/antigravity-official-acp/07-progress.md`
- 临时下载目录；不得提交 archive

**步骤**：

1. 重新读取官方 `agent.json`，确认版本、URL、cmd、args 未变化。
2. 每个 URL 下载到独立临时文件，记录最终 URL、字节数、SHA-256。
3. 列出 archive entries，确认 executable 是普通文件、相对路径安全、无路径穿越。
4. 删除临时 archive，保留命令与结果证据。

**验收**：五个 hash 都是实际文件计算的 64 位 lowercase hex；size 与下载结果一致；任一下载不稳定则本工作包 BLOCKED。

## AGACP-01：解耦 ACP connection、model 与 readiness

**目标**：空模型目录时仍保持 ACP connected/execution-ready，并让 model status 独立失败或 unsupported。

**文件白名单**：

- `src-tauri/src/backend/ai_execution/backends/acp.rs`
- `src-tauri/src/backend/agent_market/runtime.rs`
- `src-tauri/src/backend/agent_market/types.rs`
- `src-tauri/src/backend/agent_market/repository.rs`
- `src-tauri/test-fixtures/fake-acp-agent.mjs`

**先写失败测试**：

- initialize + session/new + empty config => connection success。
- 同一 installation 的 model status unsupported/failed，不改变 protocol ready。
- connected、execution_ready、registry candidate 都不依赖 model ready。
- transport/initialize/session-new failure 仍使 protocol failed。
- auth error 为 auth_required，installation 不 broken。

**实现要求**：

- connection probe 不调用 model parser。
- ACP health 先做 connection，再单独 discovery；两阶段独立写状态。
- model empty 映射为 unsupported 或专门 empty 结果，禁止写 protocol failure。
- probe cleanup 遵守 `01-contract.md` C-05；不得放松业务 OneShot cleanup。

**验证**：`cargo test --workspace` 中目标 ACP/market tests；`cargo fmt --all -- --check`。

## AGACP-02：旧 Native installation 失配治理

**目标**：catalog identity 改变后，旧 `agy` record 不进入动态 Registry，并可由用户显式 reinstall。

**文件白名单**：

- `src-tauri/src/backend/agent_market/runtime.rs`
- `src-tauri/src/backend/agent_market/repository.rs`
- `src-tauri/src/backend/agent_market/types.rs`
- `src-tauri/src/backend/application/agent_market.rs`
- `src-tauri/src/backend/runtime/app_runtime.rs`
- 对应 Rust 测试文件或同模块 tests

**先写失败测试**：

- 数据库预置 `antigravity/native/system-antigravity/agy`，活动 catalog 为 ACP/Binary；startup 后 status=incompatible。
- 该 record 不在 Registry，connected=false，execution_ready=false。
- list/preview 明确给出 reinstall 语义。
- record 的 protocol/program 未被静默改写；reinstall 成功后才替换。

**实现要求**：

- 复用 `InstallationStatus::Incompatible`。
- reconciliation 对比 protocol、distribution type 与当前 item 的可选择 distribution identity。
- 不删除系统 `agy`，不自动下载，不修改 capability assignment。
- catalog list 的 update/reinstall projection 不再只比较展示版本。

**Stop**：若需要 schema migration 或会影响其他 Agent 的合法旧安装，停止并报告精确兼容矩阵。

## AGACP-03：Catalog、证据与前端兼容元数据

**依赖**：AGACP-00、AGACP-01、AGACP-02 PASS。

**文件白名单**：

- `builtin-assets/agent-market/catalog-v1.json`
- `builtin-assets/agent-market/release-evidence-v1.json`
- `scripts/check-agent-catalog-release.mjs`
- `scripts/check-agent-catalog-release.test.mjs`
- `frontend/src/components/settings/agentCatalog.ts`
- 直接相关 frontend tests

**目标 catalog**：

- ID `antigravity`，display name `Google Antigravity`，protocol `acp`。
- upstream Registry ID `antigravity-acp`，version 1.1.1。
- 五个 Binary distributions 使用 `02-current-baseline.md` URL/entry/args 与 AGACP-00 hash/size。
- verification `experimental`。
- `textPrompt=true`；保留已验证文本 purpose；modelDiscovery 默认 false；resume/history/live/rich/team false。
- 不保留 System `agy` distribution。

**先写/更新测试**：

- 五平台 target、entry、args、hash、size 精确断言。
- release gate 对 experimental ACP 接受 partial/not_run evidence，但拒绝伪造 passed。
- legacy presentation 显示 ACP Agent。
- 旧 Native evidence 被删除，新的 evidence identity 与 catalog 匹配。

**验证**：

```bash
node --test scripts/check-agent-catalog-release.test.mjs
node scripts/check-agent-catalog-release.mjs --release
pnpm typecheck
pnpm test -- frontend/src/components/settings/agentCatalog.test.ts
```

## AGACP-04：移除 AntiGravity Native production route

**目标**：所有 `antigravity` execution 都只能按 ACP definition 进入通用 ACP backend。

**文件白名单**：

- `src-tauri/src/backend/agents/registry.rs`
- `src-tauri/src/backend/ai_execution/backends/mod.rs`
- `src-tauri/src/backend/ai_execution/backends/native.rs`
- `src-tauri/src/backend/ai_execution/backends/antigravity.rs`
- `src-tauri/src/backend/ai_execution/backends/antigravity_history.rs`
- `src-tauri/src/backend/ai_execution/backends/antigravity_tests.rs`
- `src-tauri/test-fixtures/fake-antigravity-agent`
- 与 execution capability 直接相关的 Team tests

**先写失败测试**：

- installation 生成的 Antigravity definition 为 ACP，command 是 managed Binary entry。
- Translation 选择 `antigravity` 时 ACP fake 收到 initialize/session/new/prompt。
- Native fake 没有被调用。
- catalog capability false 时 Team 入口返回能力不满足，不调用旧 Direct-CLI。

**实现要求**：

- 删除 Native backend 的 Agent ID selector 和 `agy models` 专属 parser。
- 不再编译/接线仅为 Direct-CLI production route 服务的 Antigravity 模块；可删除无 consumer 的文件与 fixture。
- 保留通用 Native backend、protocol enum 与其他 Native tests。
- 测试 builtin Registry 仅作为 fixture 更新，不把 managed path 硬编码成 production registry。
- 不修改 Conversation Adapter 或 Target Profile。

**Stop**：如果仍有已发布产品入口只能依赖 Direct-CLI 的专属 replay/live 行为，记录入口并保持 capability false；不得保留隐藏 fallback。

## AGACP-05：本地 Fake ACP 端到端回归

**目标**：以一个最高层本地接缝覆盖安装状态、动态 Registry、协议 route 与文本结果。

**文件白名单**：

- `src-tauri/test-fixtures/fake-acp-agent.mjs`
- `src-tauri/src/backend/ai_execution/backends/acp.rs`
- `src-tauri/src/backend/ai_execution/executor.rs`
- `src-tauri/src/backend/agent_market/runtime.rs`
- 直接相关 integration tests

**场景**：

1. 正常 models + prompt。
2. empty models + prompt 使用默认模型。
3. model discovery error + protocol remains ready。
4. auth error + clean process/workspace + no fallback。
5. prompt error、timeout、cancel、disconnect。
6. cleanup close/delete supported 与 unsupported。

**验收**：测试只依赖临时目录、SQLite 与 fake executable；不依赖公网、Google 登录或用户目录。

## AGACP-06：Google 官方 Binary smoke

**目标**：在当前可用平台验证真实 1.1.1 Binary。

**允许修改**：

- `builtin-assets/agent-market/release-evidence-v1.json`
- `builtin-assets/agent-market/catalog-v1.json`（仅证据支持的 modelDiscovery/verification 字段）
- `agent-docs/feature-plans/antigravity-official-acp/07-progress.md`

**流程**：install/materialize -> hash verify -> spawn -> initialize -> session/new -> inspect config -> optional set model -> prompt exact text -> collect session/update -> PromptResponse -> bounded cleanup。

Prompt：`Reply with exactly: ASSETIWEAVE_ACP_OK`

**结果规则**：

- prompt 与 cleanup 全通过，写真实 passed evidence。
- model config 非空时才把 `modelDiscovery` 改为 true。
- 无 credential 时记录 auth_required；不把 prompt 写 passed。
- session delete 不支持且留下持久记录时，本工作包 BLOCKED。
- 不开启 fs/terminal/MCP，不测试写文件。

## AGACP-07：最终门禁与审计

**目标**：执行 `04-verification-matrix.md`，回填进度与交接，不再修改行为。

**必须搜索**：

```bash
rg -n 'antigravity|agy|parse_agy_models|NativeExecutionBackend' src-tauri/src builtin-assets/agent-market frontend/src
```

每个命中必须分类为：允许的 identity/presentation、Conversation/Target 非执行资产、历史文档，或违规 production route。

**完成**：所有 Required gate PASS；真实 smoke 状态准确；未验证能力保持关闭；输出 handoff 七项。

