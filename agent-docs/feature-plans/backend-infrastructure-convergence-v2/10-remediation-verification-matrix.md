# 审计后纠偏 Verification Matrix

## G-RUST-BASE

```bash
cargo fmt --all -- --check
cargo check -p assetiweave
cargo test -p assetiweave --lib
pnpm check:boundaries
```

通过条件：全部 exit 0；当前卡修改文件不新增 compiler warning。

## Card Gates

| Gate | 必须通过的行为 | 删除/结构证据 |
|---|---|---|
| G-B2-R16 | 0ms 与短 deadline 返回时间受同一绝对 deadline 约束 | `max(Duration::from_millis(50))` 归零；abort 后 await |
| G-B2-S02 | 两个独立 SQLite fixture 的 Memory 设置互不串用；损坏 legacy JSON 不影响已有 SQLite | `_db` 忽略与业务 legacy-file fallback 归零 |
| G-B2-P03A/B | 每组生产 consumer await 同一个 async HostCommand seam | 所属同步调用点归零 |
| G-B2-P03C | normal/timeout/cancel/descendant-held-pipe/large-output/nonzero 全通过 | 同步 Command、reader thread、polling、手工 process-group FFI 归零 |
| G-B2-L02 | subscriber 输出不含绝对路径、token、secret、password、prompt、environment | `LogField` façade与 dispatcher `eprintln!` 归零 |
| G-B2-F02 | non-UTF-8 fixture 显式错误或保持 PathBuf；五个确认决策点无 lossy | 对应五文件 `to_string_lossy` 决策命中归零 |
| G-B2-D02 | null/JSON/tenant/order 行为不变 | 五个目标 store 中稳定 mapper 的位置 `try_get` 归零 |
| G-B2-G02 | 当前 SHA 的 macOS/Linux/Windows CI 全绿 | Windows fixture 确认 Job Object、KillOnDrop 与后代管道回收 |
| G-B2-G03 | 全 Contract 逐项有行为、删除和平台证据 | Issue、Router、状态索引一致 |

## G-FULL

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
node --test scripts/check-agent-catalog-release.test.mjs
node scripts/check-agent-catalog-release.mjs --release
```

通过条件：全部 exit 0；生成物无非预期 diff；测试过滤器不得为 0 tests；Issue #24 的最终评论记录当前 SHA、各平台 run URL、测试数与 warning 数。
