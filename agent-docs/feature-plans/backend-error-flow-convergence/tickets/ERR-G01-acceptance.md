# ERR-G01：错误链完整验收

**Authority:** 当前代码、行为测试、删除查询和生成 WireError contract 决定 Issue #2 完成状态。

**Files:**

- Modify: `agent-docs/feature-plans/IMPLEMENTATION-STATUS.md`
- Modify stale status only: 本计划目录
- No production changes.

## Steps

- [x] 为 `01-contract.md` 每项列出行为、source、删除和 wire parity 证据。
- [x] 重跑 `02-current-baseline.md` 四条查询；每个保留命中必须是精确私有 parser、test-only 或最外层 transport，且写出函数签名。
- [x] 确认 repository、runtime、projection 与 AppService 跨模块接口不返回 `Result<T,String>`。
- [x] 确认已知 SQLx/Serde/HostProcess/Extension 错误没有通过 `AppError::external` 或 `.to_string()` 分类。
- [x] 运行完整验证；任一失败时评论 `Status: INCOMPLETE` 并保持 Issue #2 OPEN。
- [x] 全部通过后发布矩阵、更新状态索引并关闭 Issue #2。

## Full verification

```bash
pnpm typecheck
pnpm lint --quiet
pnpm format:check
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
pnpm check:boundaries
pnpm cli:contract
git diff --exit-code -- cli/internal/schema/contract.json
pnpm gen:surface-matrix
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
cargo audit
```

状态提交：`docs(agent): 记录后端错误链完整验收`
