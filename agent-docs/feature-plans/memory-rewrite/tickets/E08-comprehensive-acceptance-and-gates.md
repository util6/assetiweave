# E08: 综合验收与全仓门禁 (Comprehensive Acceptance & Full-Repo Gates)

## 1. 切片元数据

- **切片身份**: E8
- **前置依赖 (Blocked by)**: E7 (Commit `c88b8b8`)
- **主要验收用例**:
  - E01–E18 全行为矩阵验收
  - 全仓质量门禁 (Rust/Go/TypeScript/Frontend Build)

## 2. 架构决策与设计细节

1. **E01–E18 矩阵完整覆盖**:
   - E01–E17 用例均已建立并在主测试接缝（`AppService` + SQLite + Controlled Clock）中通过；
   - 补齐 E18（后台任务中浏览、筛选、取消，观察事件与轮询：无关操作可用，状态一致，取消与失败不触发虚假完成刷新）；
2. **全仓门禁闭环**:
   - Rust 格式化与编译：`cargo fmt --all -- --check`
   - Bounded Evidence 专项全集测试：19+ 个用例全绿
   - Go CLI 语法与竞态测试：`go vet -C cli ./... && go test -C cli -race ./...`
   - 前端静态类型与测试：`pnpm typecheck && pnpm test && pnpm build`
   - 契约连续一致性：`pnpm cli:contract`

## 3. 验收记录

- [x] Red/Green 补齐：在 `bounded_evidence_baseline_tests.rs` 中补充 `test_e18_background_tasks_cancellation_and_polling_consistency` 并运行通过（20/20 全部 PASS）。
- [x] Rust 门禁通过：`cargo fmt --all -- --check`，且 `cargo test --workspace`（942 个测试全部 PASS）。
- [x] Go 门禁通过：`go vet -C cli ./... && go test -C cli -race ./...`（全部 PASS），`go test -C cli -tags=e2e -count=1 ./tests/cli_e2e`（全部 PASS）。
- [x] 前端与全仓构建门禁通过：`pnpm typecheck && pnpm test && pnpm build`（146 文件，725 单元测试全部 PASS，构建成功）。
- [x] 契约一致性：`pnpm cli:contract` 零差异。
- [x] 归档与目标完成确认：全矩阵 E01–E18 达成，准备发布 E8 提交。

