# Wave 1 工程防腐门禁基线盘点

> 对应 Wayfinder 工单：[摸底 Wave 1 防腐门禁基线：前端超限文件、Clippy/Knip/Nextest 阻塞项与 scripts/ 分层盘点](https://github.com/util6/assetiweave/issues/208)
> 地图：[工程防腐、3 亿 Token 存储扩容与契约/渲染演进规划（Wave 1~3）](https://github.com/util6/assetiweave/issues/207)
> 采集时间：2026-11-20 · 基线提交：`1d06c224`（main）
> 性质：事实清单，不做决策。决策见 Wave 1 拍板工单。

## 0. 采集边界与不可执行项（先读）

| 项 | 状态 | 说明 |
|---|---|---|
| 行数统计 | ✅ 已实测 | 用仓库现有 `countLines` 语义（去掉末尾换行后按 `\r?\n` 切分）逐文件统计 |
| 私有事件枚举 | ✅ 已实测 | 对 `desktop/src/**/*.rs` 做字符串字面量提取 + 去重 |
| `scripts/` 分层 | ✅ 已实测 | 以「package.json / .github/workflows / scripts 内部互相引用」为引用图 |
| 前端产物体积 | ✅ 已实测 | 读取本地 `dist/`（2026-11-20 09:14 构建，HEAD 为 09:18），gzip level 9 |
| `cargo clippy --workspace --all-targets` | ❌ **未执行** | 本工作环境内 `rustup` 无任何已安装 toolchain（`rustup toolchain list` → `no installed toolchains`），`cargo` 为 rustup shim，直接报 `could not choose a version of cargo`。只能用静态代理指标（§2.1） |
| `cargo-nextest` 兼容性 | ❌ **未实测** | 同因无法编译。只做了「阻塞根因」的源码层定位（§2.2） |
| 引入 `knip` | ❌ **未安装** | 需改动 lockfile，研究工单不越界。只做配置成本面盘点（§2.4） |
| 引入 `cargo-deny` | ⚠️ 部分 | `deny.toml` 不存在；重复版本可从 `Cargo.lock` 静态统计（§2.5）；advisory 库需联网，未查 |

**复现命令**（在能跑 Rust 的机器上补齐 §2.1 与 §2.2 的实测数）：

```bash
cargo clippy --workspace --all-targets --message-format=short 2>&1 \
  | grep -E "^[^ ].*warning: " | sed -E 's/.*warning: //' | sort | uniq -c | sort -rn
cargo install cargo-nextest --locked
cargo nextest run --workspace          # 去掉 --test-threads=1 后的真实结果
pnpm dlx knip                          # 先跑 dry-run 看噪声，再加配置
```

---

## 1. 全仓业务文件行限盘点

统计口径：排除 `*.test.*` / `*.spec.*` / `__tests__/` / `*_tests.rs` / `tests/` 目录。前端统计 `.ts/.tsx/.js/.jsx/.css`，Rust 统计 `.rs`。

### 1.1 `frontend/src`（342 个非测试源文件，含 2 个 `.css`）

| 阈值 | 文件数 |
|---|---|
| > 500 行（软线） | **35** |
| > 800 行（拟硬限） | **16** |
| > 1,000 行 | **13** |

超过 1,000 行的 13 个巨石文件（按行数降序）：

| 行数 | 文件 |
|---:|---|
| 3,783 | `frontend/src/components/settings/GlobalSettingsDialog.tsx` |
| 3,035 | `frontend/src/pages/conversations/ConversationsPage.tsx` |
| 2,800 | `frontend/src/services/conversations.ts` |
| 2,559 | `frontend/src/pages/prompts/PromptOverviewPage.tsx` |
| 2,074 | `frontend/src/styles/index.css` |
| 2,063 | `frontend/src/pages/mounts/SkillMountsPage.tsx` |
| 2,043 | `frontend/src/components/conversations/ConversationContentCards.tsx` |
| 1,867 | `frontend/src/components/tasks/TaskCenterModal.tsx` |
| 1,810 | `frontend/src/services/catalog.ts` |
| 1,650 | `frontend/src/pages/groups/SkillGroupsPage.tsx` |
| 1,555 | `frontend/src/manuals/registry.ts` |
| 1,252 | `frontend/src/store/settings/settingsSchema.ts` |
| 1,133 | `frontend/src/components/settings/AgentSettingsPanel.tsx` |

800~1,000 区间的 3 个（软线已破、硬限未破）：

| 行数 | 文件 |
|---:|---|
| 895 | `frontend/src/services/usage.ts` |
| 839 | `frontend/src/components/conversations/ConversationScriptResourcePanel.tsx` |
| 839 | `frontend/src/types/index.ts` |

500~800 区间的 19 个（仅列前 10，全量见末尾附录 A）：`784 AgentSessionToolDetail.tsx`、`765 MemoryRecentWorkspace.tsx`、`760 SourcesPage.tsx`、`739 DataToolbar.tsx`、`721 ModelPricingTable.tsx`、`693 ConversationSearchDialog.tsx`、`693 UsageBreakdownTables.tsx`、`631 compactActionParser.ts`、`603 mock/catalog.ts`、`595 AppSessionBrowser.tsx`。

> **与地图笔记「存量 11 个巨石文件」的口径差**：实测 >1,000 行为 13 个；排除 `styles/index.css` 与 `manuals/registry.ts` 后正好 11 个。即地图的 11 大概率是「只算 `.ts/.tsx` 业务模块」口径。**baseline 文件到底按 11 还是 13 建，需要 Wave 1 拍板时显式定口径**——两种口径会导致棘轮基线差 2 条记录。

### 1.2 `desktop/src`（76 个 `.rs`）

| 阈值 | 文件数 |
|---|---|
| > 500 行 | **5** |
| > 800 行 | **2** |
| > 1,000 行 | **0** |

| 行数 | 文件 |
|---:|---|
| 951 | `desktop/src/lib.rs` |
| 873 | `desktop/src/adapters/engine/surface_mapping.rs` |
| 649 | `desktop/src/adapters/engine/registry/catalog.rs` |
| 625 | `desktop/src/adapters/tauri/agent_market.rs` |
| 585 | `desktop/src/adapters/engine/registry/conversations/sessions.rs` |

> 关键结论：**按 1,000 行硬限，`desktop/src` 现在零违规**（951 是最高值，离硬限只差 49 行）。所以把 `desktop/src` 纳入后端行限门禁是**零存量成本**的，可以直接硬上线，不需要 baseline。但需要预留增长空间——`lib.rs` 951 行随时会破线。

### 1.3 `backend/cli/src`（43 个 `.rs`）

| 阈值 | 文件数 |
|---|---|
| > 500 行 | **1** |
| > 1,000 行 | **0** |

| 行数 | 文件 |
|---:|---|
| 523 | `backend/cli/src/service/mod.rs` |

同样零硬限违规，纳入门禁无存量成本。

### 1.4 现存门禁的真实覆盖范围

- 门禁实现：`scripts/check-backend-line-limits.mjs`，常量 `SOFT_LINE_LIMIT = 500`（第 12 行）、`MAX_LINES = 1000`（第 17 行）。
- **扫描目录只有两个**：`backend/asset_core/src`、`backend/agent_engine/src`（第 30–33 行）。`desktop/src` 与 `backend/cli/src` **不在扫描范围内**——这正是地图笔记要求补齐的点。
- 超软线只打印 notice，不失败；超硬限 `process.exit(1)`（第 83–98 行）。
- 挂载链路：`scripts/check-module-boundaries.sh:382` → `pnpm check:boundaries` → `.github/workflows/ci.yml` 的 `Check module boundaries` 步骤。
- **前端完全没有行限门禁**。`frontend/src` 当前唯一的棘轮是 native title：`scripts/native-title-baseline.json` + `frontend/src/architectureBoundaries.test.ts:233-275`。

---

## 2. CI 门禁补齐可行性摸底

### 2.1 `cargo clippy` 存量告警（未能实测，给静态代理）

| 事实 | 值 | 来源 |
|---|---|---|
| CI 是否有 clippy 步骤 | **没有** | `.github/workflows/ci.yml` 全文只有 fmt / boundaries / surface matrix / test |
| workspace 级 lint 配置 | **没有** `[workspace.lints]`，也没有任何 crate 的 `[lints]` | 全仓 `Cargo.toml` grep 无命中 |
| crate 级 `#![deny(warnings)]` | **没有** | 只有 `desktop/src/adapters/engine/surface_mapping.rs:3` 的 `#![allow(dead_code)]` |
| 存量显式 `#[allow(clippy::…)]` | **12 处，全部是 `clippy::too_many_arguments`** | 全仓 grep 去重后唯一 lint 名 |

结论口径：clippy 目前**完全未纳管**——既不在 CI 跑，也没有 workspace lint 级别声明，代码里只有 12 处针对 `too_many_arguments` 的局部豁免。所以「零告警推进」不是清理存量，而是**首次引入**，未知量是首次全量跑出来的告警总量。这个数必须实测，静态推不出来（尤其 `--all-targets` 会连测试代码一起查，165 个 asset_core 测试文件里手写 mock 的 clippy 噪声通常最大）。

### 2.2 `--test-threads=1` 的阻塞根因（源码级定位）

CI 里两处硬编码串行（`.github/workflows/ci.yml:103` 与 `:138`）。根因不是性能，是**进程级全局可变状态**：

| 根因 | 规模 | 代表位置 |
|---|---|---|
| 测试改写进程环境变量 | **56 处 `env::set_var`**，分布在 11 个文件 | `backend/asset_core/tests/conversation_hash_driven_reparse.rs:337`（`AIW_204_READ_LOG`）、各 adapter 测试的 `ASSETIWEAVE_HOME` |
| 进程级运行时单例 | 1 处 `OnceLock<Arc<AppRuntime>>`，缓存**第一个** DB 路径 | `backend/asset_core/src/infrastructure/runtime/app_runtime.rs:103` |
| 文件级互斥锁自串行 | 集成测试用 `static HOME_LOCK: Mutex<()>`；lib 内测试用 `OnceLock<Mutex<()>>` | `tests/conversation_adapter_cli_trust.rs:16`、`src/infrastructure/app_settings_tests.rs:5` |

11 个改写 env 的文件分两类：

- **集成测试二进制 7 个**（每个 `.rs` 是独立进程，跨文件本来就并行安全）：
  `conversation_adapter_dev_override_retirement.rs`、`conversation_adapter_local_promotion.rs`、`conversation_adapter_local_update_detection.rs`、`conversation_runtime_scripts_common.rs`、`conversation_adapter_builtin_source_seeding.rs`、`conversation_hash_driven_reparse.rs`、`conversation_adapter_cli_trust.rs`（均在 `backend/asset_core/tests/`）。
- **lib 内测试模块 4 个**（与同 crate 其它单测共享进程 → 真正需要互斥的原因）：
  `src/application/memory/legacy_archive_tests.rs`、`src/infrastructure/app_settings_tests.rs`、`src/infrastructure/agent_execution/agent_execution_tests.rs`、`desktop/src/adapters/engine/transport_tests.rs`。

**切换 nextest 的有利事实**：

1. nextest 的默认模型是 **process-per-test**，每个测试独立进程 → 天然隔离 `env::set_var` 与 `PROCESS_RUNTIME` 单例，这两类根因直接消失。
2. 已排查跨进程共享的固定资源：**没有**。所有测试监听端口都是 `TcpListener::bind("127.0.0.1:0")` 临时端口（如 `src/infrastructure/http_client_tests.rs:35`），不存在固定端口抢占。
3. `desktop/src` 没有独立 workspace（根 `Cargo.toml` 的 `members` 已含 `desktop`），所以 `cargo clippy/nextest --workspace` 一次覆盖四个 crate。

**风险面**：4 个 lib 内测试模块的 `OnceLock<Mutex>` 自串行在进程隔离后变成空转（需清理但不阻塞）；测试进程数从「crate 数量级」涨到「测试用例数量级」，`transport_tests.rs` 那种要起真实子进程的用例会放大总耗时——需要实测 wall-clock 才能判断提速是否成立。

### 2.3 私有 `*-task-updated` 事件白名单基线

实测 `desktop/src` 共 **12 个私有任务通道**，其中 **11 个** 匹配 `*-task-updated` glob，**1 个不匹配**：

| # | 事件名 | 出现处 | 形态 |
|---:|---|---|---|
| 1 | `batch-mount-task-updated` | `commands/mounting/batch.rs:8` | `const` + 4 处 emit |
| 2 | `git-source-pull-task-updated` | `commands/catalog/sources.rs:214` | `const` + 2 处 emit |
| 3 | `skill-backup-task-updated` | `commands/catalog/skills.rs:12` | 字面量 emit |
| 4 | `skill-remote://acquire-task-updated` | `commands/catalog/skills.rs:202,219,239,294` | 字面量 emit ×4 |
| 5 | `memory-task-updated` | `lib.rs:437`、`commands/memory/recent.rs:27,42` | 字面量 emit ×3 |
| 6 | `conversation-sync-task-updated` | `commands/conversations/sync.rs:59,115` | 字面量 emit ×2 |
| 7 | `conversation-data-maintenance-task-updated` | `commands/conversations/maintenance.rs:70,116` | 字面量 emit ×2 |
| 8 | `conversation-search-index-task-updated` | `commands/conversations/search.rs:129` | 字面量 emit |
| 9 | `conversation-script-install-task-updated` | `commands/conversations/packages.rs:27` | 字面量 emit |
| 10 | `agent-market://lifecycle-task-updated` | `agent_market.rs:24` | `const` + 8 处 emit |
| 11 | `agent-market://refresh-task-updated` | `agent_market.rs:26` | `const` + 2 处 emit |
| 12 | ⚠️ `ai-execution://task-updated` | `commands/agents/runtime.rs:12` | `const` + 1 处 emit |

**这条是本工单最值得带回拍板桌的发现**：

- 地图笔记写的是「存量 10 处」，实测是 **11 个匹配 + 1 个漏网 = 12 个通道**。
- 第 12 个 `ai-execution://task-updated` **不满足 `*-task-updated` 后缀**（它是 `//task-updated`），前端 `AiExecutionTaskProvider` 确实在监听它。如果棘轮白名单的正则按 `-task-updated$` 写，这个通道会**完全绕过门禁**，以后新增同类命名也能随便逃逸。
- 白名单必须按**精确事件名集合**而不是后缀 glob 建，且要连 `const` 定义处一起登记——否则 `batch-mount` / `git-source-pull` / `agent-market://*` 这 3 个走常量的通道在「只扫 emit 字面量」的实现里也会漏。

### 2.4 `knip` 引入成本面

未安装（`package.json` devDependencies 无 `knip`）。配置成本取决于以下几点，均已核实：

| 因素 | 现状 | 对 knip 的影响 |
|---|---|---|
| TS 路径别名 | `frontend/tsconfig.json` 有 `"@/*": ["./src/*"]`；`frontend/vite.config.ts:12-14` 同步 alias | 需在 knip 配置里重复声明 `paths`，否则全仓 `@/…` 导入被误判为未解析 |
| 入口声明 | vite 单入口 `frontend/index.html`；vitest 与源码同目录混放（`.test.tsx` 与 `.tsx` 并列） | 需显式声明 `entry` + `project`，否则测试文件会被当成「孤立文件」报无用 |
| 配置文件参与 lint | `eslint.config.mjs`、`prettier`、`tailwind`、`postcss`、`vite.config.ts`、`vitest` | 每个都要进 `ignore` 或 `entry`，否则一上来就是一堆 config 噪声 |
| 动态 import | 全仓仅 **1 处** `import.meta.glob`（`frontend/src/ecosystemAdoption.test.ts:4`） | 动态导入噪声**极低**，这是引入 knip 的有利条件 |
| 规模 | 342 个非测试源文件 | 首跑噪声主要来自 barrel/`index.ts` 再导出，需要 `--include exports` 分级放行 |

判断倾向：**动态导入噪声低，配置成本主要在别名与测试文件归类**，属于「一次性配置 30~60 行 + 首跑处理一批 exports 误报」的量级，不是深坑。但首跑到底报多少条，必须实跑才知道。

### 2.5 `cargo-deny` 引入成本面

| 事实 | 值 |
|---|---|
| `deny.toml` | 不存在 |
| `[workspace.dependencies]` 统一版本收口 | **不存在**（根 `Cargo.toml` 无该段），各 crate 各自写版本 |
| `Cargo.lock` 规模 | 726 个 package / 657 个唯一 crate |
| 存在多版本的 crate | **48 个**，其中 **19 个是 `windows-*`**（平台传递依赖，实践中在 `deny.toml` 里 `skip` 掉），**29 个是非 windows 多版本** |

非 windows 的多版本 top（这些是 `multiple-versions` 规则的真实噪声源）：

| crate | 版本 | 备注 |
|---|---|---|
| `hashbrown` | 0.12.3 / 0.15.5 / 0.16.1 / 0.17.1 | 4 个版本 |
| `syn` | 1.0.109 / 2.0.117 / 3.0.3 | proc-macro 生态常态 |
| `toml` / `toml_edit` / `toml_datetime` / `serde_spanned` / `winnow` | 各 3 个版本 | 同一升级链连带 |
| `schemars` / `schemars_derive` | 0.8.22 / 0.9.0 / 1.2.1 | 3 个版本 |
| `getrandom` | 0.2.17 / 0.3.4 / 0.4.2 | 3 个版本 |
| `thiserror` / `thiserror-impl` | 1.0.69 / 2.0.18 | 2 个版本 |
| 其余 | `base64`、`bitflags`、`digest`/`sha2`/`crypto-common`/`block-buffer`（sha2 0.10 vs 0.11 连带的整套）、`indexmap`、`png`、`jni` 等 | |

判断倾向：**`multiple-versions = "deny"` 现在一定挂**（29 个非 windows 多版本）。可行路径是先把 `multiple-versions` 设 `warn`（或 `allow` 白名单）跑一段时间，`bans`/`advisories` 直接 `deny`。advisory 检查依赖联网拉 RustSec，CI 里需考虑离线/缓存策略。

---

## 3. `scripts/` 目录分层盘点

共 **49** 个顶层文件（不含 `fixtures/`、`__pycache__/`）。分层依据：是否被 `package.json` / `.github/workflows/*.yml` / `scripts/` 内部互相引用。

### 3.1 活守卫（16，全部在 CI 或 `pnpm check:*` 链路上）

| 文件 | 被谁引用 |
|---|---|
| `check-module-boundaries.sh` | `package.json` `check:boundaries`、`ci.yml` |
| `check-module-boundaries.test.sh` | `package.json` `test:boundaries`、`ci.yml` |
| `check-backend-layer-dependencies.mjs` | `check-module-boundaries.sh`、`.test.sh` |
| `check-backend-line-limits.mjs` | `check-module-boundaries.sh`、`.test.sh` |
| `check-backend-root-layout.mjs` | `check-module-boundaries.sh`、`.test.sh` |
| `check-rust-production-pattern.mjs` | `check-module-boundaries.sh` |
| `report-backend-legacy-modules.mjs` | `check-module-boundaries.sh`、`.test.sh` |
| `lint-architecture.mjs` | `package.json` `lint:architecture`、`check-module-boundaries.sh` |
| `rust-test-files.mjs` | 5 个守卫共享的测试文件判定工具 |
| `gen-surface-matrix.sh` / `.py` | `package.json` `gen:surface-matrix`、`ci.yml` |
| `conversation-adapter-packages.mjs` | `package.json`、`conversation-adapter-packages.yml` |
| `generate-cli-contract.js` | `package.json` `cli:contract` |
| `run-go-tests.mjs` | `package.json` `cli:test:go:archive` |
| `backend-layer-dependencies.baseline.json` / `backend-legacy-modules.baseline.json` | 上述守卫的数据文件 |
| `surface-matrix-exemptions.txt` | `gen-surface-matrix.py` 数据文件 |

### 3.2 构建发布（13）

`app-icons.mjs`、`build-artifacts.mjs`、`build-cli.js`、`bundle-cli.js`、`check-agent-catalog-release.mjs`、`ensure-tauri-resources.mjs`、`install.js`、`release-audit.js`、`run.js`，外加 3 个自带测试 `build-artifacts.test.mjs` / `install.test.mjs` / `check-agent-catalog-release.test.mjs`。

### 3.3 适配器测试（8，全部挂在 `pnpm conversation-adapters:test`）

`antigravity-` / `claude-code-` / `codebuddy-` / `codex-` / `opencode-` / `web-` / `zcode-` 七个 `-conversation-adapter.test.mjs`，加 `conversation-payload-policy.test.mjs`。

### 3.4 未被任何门禁引用（12）

真正的**历史一次性验收脚本 9 个**（m2/m3/m4 阶段的一次性验收与压测，`.agents/teamwork/` 时代的产物）：

- `adversarial-m3-adapters.test.mjs`
- `antigravity-m2-adversarial-verification.mjs`
- `antigravity-m2-remediation-stress.test.mjs`
- `m3-adversarial-verification.mjs`
- `m3-challenger-navigation-stress.test.mjs`
- `m3-reviewer-adversarial-test.mjs`
- `m4-challenger-store-resilience.test.mjs`
- `memory-skill-recall.test.py`
- `shell-command-projector.test.mjs`

另外 3 个属于**「有测试但没接进任何 runner」**，和纯死脚本性质不同，处置要区别对待：

- `app-icons.test.mjs` — 对应守卫 `app-icons.mjs` 在 CI 里跑，但它的测试没人跑
- `check-agent-catalog-release.test.mjs` — 同上，守卫在 release.yml 跑，测试没人跑
- `native-title-baseline.json` — 数据文件，被 `eslint.config.mjs:9` 读取，**不是死文件**（本次扫描以「是否被 `scripts/` 外的 js/mjs 引用」为判据时漏判，特此更正）

> 结论：`scripts/` 的真实问题不是「没分层」，而是 **9 个一次性验收脚本混在守卫目录里 + 2 个守卫测试从未被执行**。前者的处置（删除 / 移入 `scripts/archive/` / 移入 `agent-docs/`）需要拍板；后者是纯收益，接进 CI 即可。

---

## 4. 前端首屏体积预算的技术切口

### 4.1 现有机制不是体积预算

`pnpm artifacts:check` 跑的是 `scripts/build-artifacts.mjs`，职责是 **`target/` + `dist/` 的磁盘占用守卫**（默认 8 GiB，`ASSETIWEAVE_BUILD_ARTIFACT_MAX_MB` 可调）。超预算时**清理**目录（`clean` 分支），不是「构建失败」。与首屏 JS 体积无关。

### 4.2 切口已经天然存在

- `package.json` 的 `postbuild` 已经是 `artifacts:check`，即**每次 `pnpm build` 后 dist 一定已生成**。新增一个 `scripts/check-frontend-budget.mjs` 只需串在 `postbuild` 前面，无需改 vite 插件、无需 source map。
- `frontend/vite.config.ts:19` 现有 `chunkSizeWarningLimit: 700`（KiB，**仅 warning 不失败**）——这是目前唯一沾边的护栏，但它看的是单个 chunk 原始体积，且不阻断。
- 棘轮模式的现成先例有 **2 个**，可以直接照抄：
  - `scripts/native-title-baseline.json` ←→ `frontend/src/architectureBoundaries.test.ts:233-275`（读 baseline，逐文件比 `actual <= limit`）
  - `scripts/backend-layer-dependencies.baseline.json` ←→ `check-backend-layer-dependencies.mjs`

### 4.3 实测基线（本地 `dist/`，2026-11-20 09:14 构建）

| 指标 | 值 |
|---|---|
| 入口 chunk `dist/assets/index-CIqJVZnc.js` | **1,666 KiB raw / 498 KiB gzip** |
| 全部 JS（121 个文件） | 5.43 MiB raw / **1.58 MiB gzip** |
| 最大分包 `mermaid.core-*.js` | 605 KiB raw / 144 KiB gzip |
| 次大分包 `wardley-*.js` | 601 KiB raw / 145 KiB gzip |
| 第三 `cytoscape.esm-*.js` | 433 KiB raw / 139 KiB gzip |

**最该被预算锁住的数是入口那 498 KiB gzip**：入口 chunk 1.67 MB raw 意味着首屏把绝大部分应用代码和至少一个重型依赖都塞进了单一 chunk，代码分割基本没起作用（mermaid / cytoscape 虽然已分包，但入口本身仍是巨型单块）。任何「首屏 gzip 上限」规则若只锁总 JS 体积（1.58 MiB）会形同虚设——必须**单独锁入口 chunk**。

---

## 5. 给 Wave 1 拍板工单的输入清单

1. **行限口径必须先定 11 还是 13**。前端 baseline 差 2 条记录（CSS 与 `manuals/registry.ts` 算不算业务文件）。
2. **`desktop/src` 与 `backend/cli/src` 可以直接上硬限 1000，无需 baseline**——实测零违规，最高 951 行。但 `lib.rs` 只剩 49 行余量，需同时决定是否给 desktop 也建 baseline。
3. **私有事件白名单必须用精确事件名集合，不能用 `*-task-updated` 后缀匹配**。实测 12 个通道，后缀匹配会漏掉 `ai-execution://task-updated`；只扫 emit 字面量会漏掉 3 个走 `const` 的通道。
4. **clippy 是首次引入而非清理存量**，且首次全量告警数未知（本环境跑不了）。建议施工工单里第一步就是「跑一次全量 clippy，把告警分布落盘」，再定零告警的推进节奏。
5. **nextest 的技术可行性高、收益待实测**：per-test 进程模型正好消灭仅有的两类阻塞根因（env 改写 + `PROCESS_RUNTIME` 单例），且无固定端口等跨进程共享资源。风险在总 wall-clock（子进程型测试被放大）。
6. **`scripts/` 治理的真实收益点是「2 个守卫测试从没跑过」和「9 个一次性脚本混在守卫目录」**，不是重新逐层拍照。
7. **首屏预算要锁入口 chunk（当前 498 KiB gzip），不能只锁总 JS**。切口是 `postbuild` 里串一个新守卫 + baseline json，与现有两个棘轮先例同构。

---

## 附录 A：`frontend/src` 500~800 行区间全量（19 个）

| 行数 | 文件 |
|---:|---|
| 784 | `components/agent-session/AgentSessionToolDetail.tsx` |
| 765 | `components/memory/MemoryRecentWorkspace.tsx` |
| 760 | `pages/sources/SourcesPage.tsx` |
| 739 | `components/common/DataToolbar.tsx` |
| 721 | `components/settings/ModelPricingTable.tsx` |
| 693 | `components/conversations/ConversationSearchDialog.tsx` |
| 693 | `components/conversations/usage/UsageBreakdownTables.tsx` |
| 631 | `components/conversations/compactActionParser.ts` |
| 603 | `mock/catalog.ts` |
| 595 | `components/conversations/AppSessionBrowser.tsx` |
| 580 | `hooks/catalog/useCatalogController.ts` |
| 574 | `components/groups/SkillGroupEditDialog.tsx` |
| 546 | `components/memory/recent/recentModel.ts` |
| 534 | `app/backgroundTasks/ConversationSearchProvider.tsx` |
| 526 | `components/sources/SourceImportDialog.tsx` |
| 516 | `components/foundation/TruncatedText.tsx` |
| 515 | `components/layout/ResizableColumns.tsx` |
| 512 | `components/settings/ConversationPricingSettingsPanel.tsx` |
| 506 | `services/agentRuntime.ts` |

（校验：35（>500 全量） − 13（>1,000） − 3（800~1,000） = 19）
