# 交接记录：AGACP-03

## 基本信息

- Task ID：AGACP-03
- Commit：`62d1177`
- 执行日期：2026-09-07
- 基线 HEAD：`450f328`
- 工作区原有修改：无

## 1. 修改文件列表

| 文件 | 修改目的 |
|---|---|
| `builtin-assets/agent-market/catalog-v1.json` | 迁移 `antigravity` 为官方 ACP Server（protocol: acp, version: 1.1.1, upstream: antigravity-acp, verification: experimental, 包含 5 平台官方 binary distributions，移除旧 system agy） |
| `builtin-assets/agent-market/release-evidence-v1.json` | 移除旧 native evidence，新增 5 个平台官方 ACP binary 的 release evidence 记录（状态为 experimental，未跑项注明 not_run），并更新 `catalogContentSha256` |
| `scripts/check-agent-catalog-release.mjs` | 增加防伪造 ACP conformance 门禁：状态为 `passed` 的 conformance 不能包含 incomplete 或未通过的 step |
| `scripts/check-agent-catalog-release.test.mjs` | 增加 Antigravity 5 平台 target/url/entry/args/hash/size 精确断言测试，以及防伪造 passed conformance 测试 |
| `frontend/src/components/settings/agentCatalog.ts` | 更新 `legacyPresentationItems` 中 Antigravity 为 `"ACP Agent"` |
| `frontend/src/components/settings/agentCatalog.test.ts` | 新增前端测试，验证 legacyPresentationItems 与 marketItemToCatalogItem 投影 |
| `src-tauri/src/backend/agent_market/catalog.rs` | 更新 `bundled_catalog_contains_all_initial_agents_without_execution_commands` 测试断言，校验 bundled catalog 中 Antigravity 为 1.1.1 ACP 协议与 5 个 binary distribution |

## 2. 测试先行证据

- 新增/修改测试：
  - `scripts/check-agent-catalog-release.test.mjs`: `bundled catalog antigravity item satisfies official ACP specification across 5 platforms` 与 `release gate rejects fraudulent passed ACP conformance evidence`。
  - `frontend/src/components/settings/agentCatalog.test.ts`: 验证 antigravity presentation 为 `ACP Agent` 及 marketItem 投影。
  - `src-tauri/src/backend/agent_market/catalog.rs`: `bundled_catalog_contains_all_initial_agents_without_execution_commands`。
- 施工前失败命令：
  - `node --test scripts/check-agent-catalog-release.test.mjs`（失败：`antigravity.protocol === 'native' !== 'acp'`）。
  - `pnpm test -- frontend/src/components/settings/agentCatalog.test.ts`（失败：Expected `"ACP Agent"`, Received `"Native Agent"`）。
- 精确失败原因：Catalog 尚未迁移为 ACP 1.1.1 协议及 5 平台 binary，前端 legacy presentation 仍标为 Native Agent。
- 实现后通过命令：
  - `node --test scripts/check-agent-catalog-release.test.mjs && node scripts/check-agent-catalog-release.mjs --release` (12 passed, 0 failed)。
  - `pnpm test -- frontend/src/components/settings/agentCatalog.test.ts` (145 files passed, 713 passed)。
  - `CARGO_TARGET_DIR=target/test-target cargo test -p assetiweave --lib -- backend::agent_market::catalog::tests` (7 passed, 0 failed)。

## 3. 验证结果

| 命令 | PASS/FAIL | 摘要 |
|---|---|---|
| `node --test scripts/check-agent-catalog-release.test.mjs` | PASS | 12 passed, 0 failed |
| `node scripts/check-agent-catalog-release.mjs --release` | PASS | Agent catalog release check passed: 7 items |
| `pnpm typecheck` | PASS | TypeScript 类型检查通过 |
| `pnpm test` | PASS | 前端 145 个测试套件，713 个测试全部通过 |
| `cargo test -p assetiweave --lib -- backend::agent_market::catalog::tests` | PASS | 7 passed, 0 failed |
| `go vet -C cli ./... && go test -C cli -race ./...` | PASS | Go CLI 静态检查与测试通过 |

## 4. 契约核对

- [x] logical ID 仍为 `antigravity`
- [x] upstream Registry ID 为 `antigravity-acp`，version 为 `1.1.1`
- [x] 5 个 Binary distributions 使用真实 URL/entry/args 与锁定 hash/size
- [x] 不保留 System `agy` distribution
- [x] verification 为 `experimental`，未完成真实 E2E 绝不标记为 `tested`
- [x] conformance 分步如实标记 `not_run`，拒绝伪造 `passed`
- [x] `textPrompt=true`；保留已验证文本 purpose；modelDiscovery 默认 false；resume/history/live/rich/team false
- [x] Conversation Adapter 无改动
- [x] Target Profile 无改动
- [x] Native 通用 seam 保留

## 5. 真实 AntiGravity ACP E2E

- 状态：NOT_RUN（保留在 AGACP-06/07）
- 观察结果：元数据与门禁严格对齐，待进入后续执行路由重构与真实联调。
