# Verification Matrix

## 共享 Gate

### G-RUST-BASE

```bash
cargo fmt --all -- --check
cargo check -p assetiweave
cargo test -p assetiweave --lib
pnpm check:boundaries
```

通过：四条命令 exit 0；若 `cargo check` 有 warning，数量不高于 B2-00 基线，且本卡修改模块无新增 warning。

### G-SURFACES

```bash
pnpm cli:contract
git diff --exit-code -- cli/internal/schema/contract.json || git diff -- cli/internal/schema/contract.json
pnpm gen:surface-matrix
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
```

通过：生成物由命令产生；diff 只包含卡片声明的 async 内部变化所必需的公开契约变化。若公开契约无变化，生成文件必须无 diff。

### G-FULL

```bash
pnpm typecheck
pnpm lint --quiet
pnpm format:check
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
pnpm check:boundaries
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

通过：全部 exit 0；测试数与 warning 数写入 B2-G01 交接。

## Ticket Gates

| Gate | Ticket | 行为证据 | 删除/结构证据 |
|---|---|---|---|
| G-B2-00 | B2-00 | 当前全量测试与命中分类 | 每个残余命中归属一张卡 |
| G-B2-R01 | B2-R01 | ResidentHost/OneShot lifecycle seam | 守卫禁止新增 runtime bridge |
| G-B2-S01 | B2-S01 | Settings round-trip/default/type/migration | 生产设置读取不再链式猜测核心字段 |
| G-B2-R02 | B2-R02 | outbox、offset、failure isolation、wake、cancel | dispatcher 无 thread/Condvar/detach |
| G-B2-R03 | B2-R03 | Engine async dispatch 保持 method/risk/exposure/error | registry 不再要求同步 handler |
| G-B2-R04 | B2-R04 | 设置/系统/租户跨 Tauri/Engine 行为 | 本领域内部 bridge 为零 |
| G-B2-R05 | B2-R05 | Asset/Source/Profile/Catalog workflow parity | 本领域内部 bridge 为零 |
| G-B2-R06 | B2-R06 | Mount/Group/Skill/Backup workflow parity | 本领域内部 bridge 为零 |
| G-B2-R07 | B2-R07 | Adapter catalog/sync/acquisition parity | 本领域内部 bridge 为零 |
| G-B2-R08 | B2-R08 | Conversation record/search/maintenance parity | 本领域内部 bridge 为零 |
| G-B2-R09 | B2-R09 | Session/Project/Global Memory parity | 本领域内部 bridge 为零 |
| G-B2-R10 | B2-R10 | Recall/Search/Public Memory parity | 本领域内部 bridge 为零 |
| G-B2-R11 | B2-R11 | Team lifecycle parity | 本领域内部 bridge 为零 |
| G-B2-R12 | B2-R12 | Agent/Market/AI/HTTP lifecycle parity | 本领域内部 bridge 为零 |
| G-B2-R13 | B2-R13 | async bootstrap、同池、角色差异 | Database 仅 pool；backend runtime bridge 为零 |
| G-B2-R14 | B2-R14 | 一个 deadline 下的完整 shutdown report | 无 detach worker；pool close 有 timeout |
| G-B2-P01 | B2-P01 | 本地 process-wrap/which fixture | 候选 API/feature/platform 记录完整 |
| G-B2-P02 | B2-P02 | HostProcess 全行为 fixture 与 consumer 回归 | 手工进程树/taskkill/重复 reader lifecycle 删除 |
| G-B2-L01 | B2-L01 | span、rolling、snapshot、stdout 隔离、脱敏 | 旧 façade 无生产 consumer |
| G-B2-F01 | B2-F01 | 三平台 path fixtures、非 UTF-8 OS path | 应用目录拼装与 lossy 决策路径删除 |
| G-B2-D01 | B2-D01 | repository null/JSON/tenant/order 行为 | 稳定重复 `try_get` 显著下降 |
| G-B2-Q01 | B2-Q01 | wire error parity、dependency audit | 重复取消 variant 为零、触及模块 warning 为零 |
| G-B2-G01 | B2-G01 | G-FULL + 桌面/Engine smoke | 所有 Contract 删除证据为零或有明确保留理由 |

## 目标测试命令

卡片在目标测试存在时直接使用以下命令；重命名测试必须同时更新卡片和本矩阵。

```bash
cargo test -p assetiweave backend::runtime::tests -- --nocapture
cargo test -p assetiweave backend::events -- --nocapture
cargo test -p assetiweave backend::host_process -- --nocapture
cargo test -p assetiweave backend::app_settings -- --nocapture
cargo test -p assetiweave backend::logs -- --nocapture
cargo test -p assetiweave backend::host_paths -- --nocapture
cargo test -p assetiweave backend::host_filesystem -- --nocapture
cargo test -p assetiweave backend::store -- --nocapture
```

通过：目标测试 exit 0 且至少执行一项目标测试；0 tests 不计通过。新回归名称和数量写入 Issue 交接。
