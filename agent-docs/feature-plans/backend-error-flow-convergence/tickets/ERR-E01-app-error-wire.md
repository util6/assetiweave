# ERR-E01：收紧 AppError 与 WireError 边界

**Authority:** AppError 分类内部失败；WireError 独立表达公开稳定契约。

**Files:**

- Modify/Test: `src-tauri/src/backend/runtime/error.rs`
- Modify/Test only when parity behavior requires it: `src-tauri/src/adapters/tauri/commands.rs`
- Modify/Test: `src-tauri/src/adapters/engine/transport.rs`
- Modify compiler-reported AppError conversion callers only.

## Steps

- [ ] 统计 `Storage(String)`、`Process(String)`、`External(String)`、`Extension(String)` 与 `Domain { code }` 的生产构造点，确认前序卡已经提供 typed source。
- [ ] 增加 taxonomy table test，逐个规范分类验证 code、retryable、message、details、source 和 Tauri/Engine parity。
- [ ] 增加绝对路径、SQL、token、secret、password、prompt、environment 的公开序列化测试。
- [ ] 将已知基础设施 String variant 替换为 typed `#[from]`/`#[source]` variant；未知 internal/external 分支保留 source，不接受仅 Display 字符串。
- [ ] 将 `AppError -> WireError` 转换固定在 adapter 使用的显式 API；删除依赖 AppError 任意内部形状的直接序列化。
- [ ] `Domain` 只保留协议透传确需的已验证 code；普通生产构造点迁入规范 variant。
- [ ] 重新生成 Engine contract；WireError schema 和既有 code 无 diff。

## Verify

```bash
cargo test -p assetiweave backend::runtime::error -- --nocapture
cargo test -p assetiweave error_taxonomy_tauri_engine_parity -- --nocapture
pnpm cli:contract
git diff --exit-code -- cli/internal/schema/contract.json
pnpm gen:surface-matrix
pnpm check:surface-matrix
go test -C cli -race ./...
pnpm cli:test:e2e
```

提交：`refactor(errors): 收紧应用错误与传输错误边界`
