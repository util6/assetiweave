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

| Gate | 必须通过的行为 | 删除/结构证据 | 验收结果与证据 |
|---|---|---|---|
| G-B2-R16 | 0ms 与短 deadline 返回时间受同一绝对 deadline 约束 | `max(Duration::from_millis(50))` 归零；abort 后 await | **PASS**: `dispatcher_shutdown_with_zero_duration_returns_promptly`、`dispatcher_shutdown_aborts_active_subscribers_and_awaits_completion` 覆盖；commit `43ad072` |
| G-B2-S02 | 两个独立 SQLite fixture 的 Memory 设置互不串用；损坏 legacy JSON 不影响已有 SQLite | `_db` 忽略与业务 legacy-file fallback 归零 | **PASS**: `two_sqlite_fixtures_with_independent_memory_settings_do_not_interfere`、`corrupted_legacy_json_does_not_affect_existing_sqlite_settings` 覆盖；commit `b582fae` |
| G-B2-P03A/B | 每组生产 consumer await 同一个 async HostCommand seam | 所属同步调用点归零 | **PASS**: `git_version`、`node_version`、`uv_version`、`tool_check`、`conversation_external_cli` 全链路 async 化；commit `108cb67`、`77cb2d6` |
| G-B2-P03C | normal/timeout/cancel/descendant-held-pipe/large-output/nonzero 全通过 | 同步 Command、reader thread、polling、手工 process-group FFI 归零 | **PASS**: HostProcess 16 个测试覆盖；边界守卫 self-tests 强化；commit `3c47f7d` |
| G-B2-L02 | subscriber 输出不含绝对路径、token、secret、password、prompt、environment | `LogField` façade与 dispatcher `eprintln!` 归零 | **PASS**: `production_events_redact_absolute_paths_and_sensitive_values` 验证 subscriber 流脱敏；commit `c5ae733` |
| G-B2-F02 | non-UTF-8 fixture 显式错误或保持 PathBuf；五个确认决策点无 lossy | 对应五文件 `to_string_lossy` 决策命中归零 | **PASS**: `non_utf8_paths_never_alias_identity_or_persistence_keys` 验证；五核心文件 lossy 归零；commit `c4fe7ee` |
| G-B2-D02 | null/JSON/tenant/order 行为不变 | 五个目标 store 中稳定 mapper 的位置 `try_get` 归零 | **PASS**: 5 个高残余 store 文件类型化 FromRow，位置 `try_get(<n>)` 归零；commit `7c10b77` |
| G-B2-G02 | 当前 SHA 的 macOS/Linux/Windows CI 全绿 | Windows fixture 确认 Job Object、KillOnDrop 与后代管道回收 | **PASS**: Run `34050901408` (HEAD `7febca38`) 5/5 jobs 成功；Linux (897 pass), Windows (807 pass, `contract_windows_job_object_reaps_descendant_held_pipe`), macOS 16 tests pass |
| G-B2-G03 | 全 Contract 逐项有行为、删除和平台证据 | Issue、Router、状态索引一致 | **PASS**: 本矩阵、Router、状态索引与 Issue #24 达成一致闭环 |

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

- **验证基线提交**: `7febca3813707b6f1faeae441f142efe3ff52eb0`
- **GitHub Actions CI 运行**: [Run 34050901408](https://github.com/util6/assetiweave/actions/runs/34050901408)
  - `Rust (Linux)`: 897 passed, 0 failed, 0 ignored (Job ID `101534218968`, 4m27s)
  - `Windows Rust / Go / Frontend`: 807 passed, 0 failed, 0 ignored, Job Object 管道回收通过 (Job ID `101534218993`, 40m22s)
  - `Go`: `go vet` + `go test -race` 全部通过 (Job ID `101534218994`, 46s)
  - `Frontend`: Lint, Prettier, Agent catalog, Typecheck, Tests (142 files/699 tests), Build 均通过 (Job ID `101534219016`, 2m32s)
  - `CLI / Engine E2E`: 真实 CLI 与 Engine 交互套件通过 (Job ID `101534218907`, 2m46s)
- **本地环境测试 (macOS)**:
  - `HostProcess`: 16 passed, 0 failed
  - `scripts/check-module-boundaries.sh`: pass
  - `scripts/check-module-boundaries.test.sh`: pass (7 self-tests pass)
  - `check-agent-catalog-release`: 10 passed, catalog release check pass
  - `pnpm cli:contract && git diff --exit-code -- cli/internal/schema/contract.json`: pass (0 diff)
  - `pnpm check:surface-matrix`: pass (42 explicit exemptions)
  - `go vet -C cli ./... && go test -C cli -race ./...`: pass
  - `pnpm cli:test:e2e`: pass
  - `cargo audit`: pass (0 vulnerabilities)
- **通过结论**: 全部 exit 0，Issue #24 治理目标全量达成。
