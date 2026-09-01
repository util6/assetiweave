# LUNA-01：架构收口真实基线

- 日期：2026-08-21
- 审计基线：`f1cd7c6` 之后的当前工作区
- 工作包：只修正文档和 Catalog release 测试基线，不修改运行时行为
- 状态：完成

## 1. 直接失败证据

施工前把 Catalog release 测试的固定断言临时推进到下一 revision
`2026.08.22.1`，未改变当前 Catalog。测试失败：

```text
expected: /7 items, catalog 2026\\.08\\.22\\.1/
actual:   Agent catalog release check passed: 7 items, catalog 2026.08.21.1
```

这证明测试绑定展示 revision 常量，而不是验证 Catalog 当前自身的 revision 规则。
测试现已从 bundled Catalog 读取 item 数量和 `catalogVersion`，后续 revision 更新无需修改断言常量。

## 2. Phase 22 历史状态校正（2026-08-21）

| Task | Status | Evidence | Reason |
|---|---|---|---|
| 22.1 | achieved | `scripts/check-agent-catalog-release.test.mjs`、Agent Market tests | Core/version 观测语义、cache 选择和生命周期门禁已有直接测试；本包同时消除 release 测试的陈旧 revision 断言。 |
| 22.2 | achieved | `pnpm check:boundaries`、Application `LegacyResult` negative search | Application workflow 已离开 DTO String alias；更底层兼容 seam 仍由 LUNA-09 继续收口。 |
| 22.3 | achieved | `pnpm check:boundaries` runtime dependency 与 DTO alias self-tests | Runtime 不反向依赖 Application，正式边界不再使用 DTO 旧结果别名。 |
| 22.4 | incomplete | `src-tauri/src/adapters/tauri/background_tasks.rs` 仍是领域任务投影接缝；缺少 Source scan/batch mount 的最高层行为证据 | TaskRuntime 已接线，但尚未证明所有长任务在启动、取消、去重和终态上共享同一生产 workflow。 |
| 22.5 | incomplete | HostProcess/Extension Kernel 类型和局部 tests 存在；缺少完整 probe 错误分类的跨边界证据 | 新 Kernel consumer 已出现，但统一 process probe、snapshot 和错误转换尚未由 E3/E4 行为测试闭合。 |
| 22.6 | contradicted | `TargetCatalog` 动态 Provider 证据主要位于孤立单元测试；生产刷新失败保留旧 snapshot 尚无直接测试 | “Provider-neutral 动态闭环”不能由 `builtin_for_tests` 或局部 descriptor 测试推断完成。 |
| 22.7 | achieved | `legacy_gemini`、旧 structured executor 符号 negative search；boundary self-test | 旧 Agent executor 生产路径已删除或委托 canonical runtime。 |
| 22.8 | contradicted | `rg 'agent_market_lifecycle_e2e|fake.*ACP' src-tauri` 未发现对应完整 E2E 场景 | 现有 lifecycle tests 不能证明 install/update/failed-update/restart-recovery/cancel 全路径。 |
| 22.9 | achieved | `scripts/check-module-boundaries.test.sh`、`pnpm check:boundaries` | 守卫包含违规 fixture self-test 与精确检查。 |
| 22.10 | incomplete | `node scripts/check-agent-catalog-release.mjs --static` 与 release fixture gate 可通过；当前证据未覆盖可复现的真实 package/network release 验证 | 静态 evidence 不等于真实 package/release E5 证据。 |
| 22.11 | missing evidence | 当前工作区已有后续改动，且 SPEC-BA-07 全量 requirement matrix 尚未逐项记录 | 既有全量命令通过不能自动证明当前工作区和每个矩阵表项仍然通过。 |

## 3. 后续工作包入口

以下工作包保持未完成，按 SPEC-BA-10 串行执行：

1. LUNA-02：同一常驻进程内租户上下文原子切换。
2. LUNA-03：搜索索引任务单生命周期、去重和终态收敛。
3. LUNA-04：Engine 成功 no-op 与 OneShot 跨调用任务契约。
4. LUNA-05 至 LUNA-10：Settings、长任务前端、BatchMount、retention、结构化错误和 TargetCatalog 动态闭环。

在前三个 P1 工作包完成并审计签署前，不得将 Phase 22 恢复为整体完成，也不得开始 Settings、Batch Mount 或 TargetCatalog 的并行大范围施工。

## 4. 2026-09-01 当前实现复核

本节覆盖上表之后已合入主干的实现与验证，不改写当日审计快照。当前 Phase 22 相关目标
已通过生产 consumer、回归测试和跨层质量门闭合：

| 范围 | 当前证据 | 结果 |
|---|---|---|
| TaskRuntime 与长任务 | `src-tauri/src/adapters/tauri/background_tasks.rs`、全量 Rust tests | PASS |
| HostProcess/Extension Kernel | ACP/Native runtime、错误边界、清理和取消测试 | PASS |
| TargetCatalog | 动态 provider、非法刷新保留旧 snapshot、seed/detect/plan/mount 测试 | PASS |
| Agent lifecycle | install/update/failure recovery/cancel lifecycle E2E | PASS |
| Release evidence | static、network release、real ACP binary E2E | PASS |
| Interface/contract | `pnpm check:boundaries`、`pnpm test:boundaries`、`pnpm check:surface-matrix` | PASS |

当前汇总提交为 `bc5c14e`。因此本文开头的“完成”结论以本节和
`agent-docs/feature-plans/IMPLEMENTATION-STATUS.md` 为准；第 2 节仅作为历史审计基线。

## 5. 本包验收

```bash
node --test scripts/check-agent-catalog-release.test.mjs
node scripts/check-agent-catalog-release.mjs --static
git diff --check
```
