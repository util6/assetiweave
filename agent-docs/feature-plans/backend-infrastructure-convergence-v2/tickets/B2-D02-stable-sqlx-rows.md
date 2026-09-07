# B2-D02：剩余高频 SQLx Stable Rows

**Authority:** 稳定多列查询由私有 `#[derive(sqlx::FromRow)]` row struct 映射；repository 只负责领域验证与 JSON/枚举转换。

**Contract:** C-SQLX-01。

**Files:**

- Modify/Test: `src-tauri/src/backend/store/global_memory_repo.rs`
- Modify/Test: `src-tauri/src/backend/store/project_memory_repo.rs`
- Modify/Test: `src-tauri/src/backend/store/search_index_repo.rs`
- Modify/Test: `src-tauri/src/backend/store/memory_recall_repo.rs`
- Modify/Test: `src-tauri/src/backend/store/menu_repo.rs`

## Steps

- [ ] 记录五文件 `try_get` 基线，当前应分别为 61、54、48、27、24。
- [ ] 列出每个固定三列以上 SELECT 和复用 mapper；标量、运行时动态列名才允许保留手工读取。
- [ ] 在修改 mapper 前补齐 nullable、非法历史 JSON、未知 enum、tenant isolation、稳定排序和空结果行为测试。
- [ ] 运行目标测试，保存 GREEN 基线；本卡是结构迁移，RED 来自新加的结构守卫：稳定 mapper 中出现位置 `try_get(<number>)` 时失败。
- [ ] 为每组查询增加私有 FromRow 类型并使用具名 SQL alias；JSON 和 enum 在 fetch 后显式验证。
- [ ] 将 `row.try_get(...).map_err(AppError::external)` 改为 typed SQLx source；不得把 SQLx 错误字符串化。
- [ ] 删除固定 mapper 的位置索引；保留命中必须在交接中列出查询、动态原因和行为测试。

## Verify

```bash
cargo test -p assetiweave backend::store -- --nocapture
cargo test -p assetiweave memory -- --nocapture
cargo test -p assetiweave search_index -- --nocapture
cargo fmt --all -- --check
pnpm check:boundaries
```

完成条件：五文件固定 mapper 的 `try_get(<数字>)` 为零；全部保留命中有具体动态/标量原因。

提交：`refactor(store): 类型化剩余高频 SQLx 行映射`
