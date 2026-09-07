# B2-D01：SQLx typed row 接管重复行映射

**Objective:** 用 `FromRow/query_as` 删除稳定查询的机械 `try_get`，保持 SQL 与业务语义可见。

**Contracts:** C-SQLX-01、C-PRODUCT-01、C-ERROR-01。

**Canonical authority after this card:** 每个稳定查询的私有 row struct 负责 SQL→Rust 映射；repository 负责领域转换。

**Files:**

- Modify: `src-tauri/src/backend/store/conversation_repo.rs`
- Modify: `src-tauri/src/backend/store/session_memory_repo.rs`
- Modify: `src-tauri/src/backend/store/source_repo.rs`
- Modify: `src-tauri/src/backend/store/team_repo.rs`
- Modify: `src-tauri/src/backend/store/web_record_repo.rs`
- Modify: corresponding repository behavior tests.

## Steps

- [ ] From B2-00 counts, list every fixed-shape query in the five files that manually reads three or more columns or repeats the same mapping.
- [ ] Add behavior tests before mapping changes for nullable fields, malformed/historical JSON, tenant isolation, stable ordering and empty result semantics.
- [ ] Define private `#[derive(sqlx::FromRow)]` row types next to the owning repository query group; field names match SQL aliases.
- [ ] Convert selected queries to `query_as`; keep JSON decoding and domain validation explicit after row fetch.
- [ ] Keep scalar/dynamic-column queries on `query_scalar` or explicit Row access; annotate each retained `try_get` category in the handoff, not in production comments.
- [ ] Remove positional indexes from converted queries and run each repository's full behavior tests.

## Tests

```bash
cargo test -p assetiweave backend::store -- --nocapture
cargo test -p assetiweave backend::conversations -- --nocapture
cargo test -p assetiweave memory -- --nocapture
cargo test -p assetiweave team -- --nocapture
```

## Delete proof

```bash
for file in conversation_repo.rs session_memory_repo.rs source_repo.rs team_repo.rs web_record_repo.rs; do
  printf '%s ' "$file"
  rg -c 'try_get' "src-tauri/src/backend/store/$file" || true
done
```

通过：所有已列 fixed-shape/repeated mappings 均迁移；剩余命中只属于 scalar/dynamic/intentional categories，且高层行为测试绿。

**Stop:** 不改 SQL 过滤、排序或事务边界来方便 derive；需要查询语义变化时拆新 Issue。

**Commit:** `refactor(store): 扩大 SQLx 类型化行映射`

