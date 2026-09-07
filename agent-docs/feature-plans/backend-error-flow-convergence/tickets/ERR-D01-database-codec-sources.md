# ERR-D01：数据库与 Codec Typed Source

**Authority:** `sqlx::Error` 和 `serde_json::Error` 保留到 AppError 映射点；repository 不把它们压成字符串。

**Files:**

- Modify/Test: `src-tauri/src/backend/runtime/error.rs`
- Modify/Test: `src-tauri/src/backend/store/codec.rs`
- Modify: `src-tauri/src/backend/store/settings_repo.rs`
- Modify: ERR-00 分类为 database/codec 的 store 文件；单提交超过 12 个生产文件时按 store 名字母顺序拆成连续提交，每个提交都运行本卡 Gate。

## Steps

- [x] 增加 `sqlx_row_error_preserves_database_source_and_wire_code`：构造列类型不匹配，断言 source chain 包含 SQLx、wire code 为 `storage_error`、公开 message 不含 SQL。
- [x] 增加 `stored_json_error_preserves_codec_source_without_public_payload`：读取非法持久 JSON，断言内部 source 为 Serde、公开 message/details 不含原始 JSON。
- [x] 运行新测试，确认当前 `AppError::external` 产生错误分类或丢失 source 的 RED。
- [x] 为持久 codec 定义 `thiserror` typed error，区分 encode、decode 和 schema validation；通过 `#[from]` 进入 AppError 的 storage 分支。
- [x] 所有 `row.try_get` 直接使用 `?` 或 `map_err(AppError::Db)`；Serde 使用 typed codec conversion。
- [x] 删除 database/codec 分类内的 `map_err(AppError::external)` 和 `.to_string()` 往返。

## Verify

```bash
cargo test -p assetiweave sqlx_row_error_preserves_database_source_and_wire_code -- --nocapture
cargo test -p assetiweave stored_json_error_preserves_codec_source_without_public_payload -- --nocapture
cargo test -p assetiweave backend::store -- --nocapture
cargo fmt --all -- --check
pnpm check:boundaries
```

提交：`refactor(errors): 保留数据库与持久文档错误来源`
