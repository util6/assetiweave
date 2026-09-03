# A-R12：SQLx typed row 接管机械数据库映射

> **Status: PLANNED**。使用 `superpowers:executing-plans`。

**Goal:** 用已有 FromRow/query_as 删除重复/位置式解码，SQL/事务/租户和历史 JSON 语义不变。
**Depends:** A-R04。
**Contracts:** C-BASE、C-STORAGE、C-ERROR。
**Gates:** G-RUST、G-BEHAVIOR。

## 文件与接口

- Modify/Test: `src-tauri/src/backend/store/team_repo.rs`、`src-tauri/src/backend/store/source_repo.rs`。
- Read: `src-tauri/src/backend/store/sql.rs`、`store/codec.rs`、`models/assets.rs`、`models/team.rs`；省略路径均位于 backend。
- Create: 各 repo 内私有 `TeamMailboxRow`、`SourceRow`，`#[derive(sqlx::FromRow)]`；一次 `TryFrom<SourceRow> for Source` 负责领域 normalize/JSON，`From<TeamMailboxRow> for TeamMailboxMessage` 负责字段投影。
- 外部 repo 函数签名不变，模型不新增数据库连接/行为；不引新 ORM，不改 migration，不使用需要在线数据库的 query! 宏。

TeamMailboxRow 的完整字段与当前 SELECT 对应：`id/team_id/run_id/sender_member_id/recipient_member_id/message_type/body/created_at:String`，`task_id/read_at/acked_at:Option<String>`。SourceRow 按 sql.rs 的命名列：id/name/kind/root_path/scanner_kind/source_origin/scan_root/include_globs/exclude_globs 为 String，repo_root/origin_app_kind/origin_provider_id/default_kind/last_scanned_at/last_scan_status 为 Option<String>，enabled 为 i64，priority 为 i32（当前 Source.priority）。用列名 derive，不保留 try_get(0..16) 作为 fallback。

## 核心接管

```rust
let rows = sqlx::query_as::<_, TeamMailboxRow>(query)
    .bind(tenant_id).bind(&input.team_id).bind(&input.run_id)
    .bind(&input.recipient_member_id)
    .fetch_all(pool).await.map_err(AppError::external)?;
Ok(rows.into_iter().map(Into::into).collect())
```

仅替换 read_team_mailbox 的读取部分；前面的 ack/read UPDATE、过滤/排序和写后语义保持。send_team_mailbox 的幂等 SELECT 使用同一 Row，删除第二份手写字段构造。

```rust
#[tokio::test]
async fn typed_mailbox_row_preserves_nulls() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
        .connect("sqlite::memory:").await.unwrap();
    let row = sqlx::query_as::<_, TeamMailboxRow>(
        "SELECT 'm' AS id, 't' AS team_id, 'r' AS run_id, NULL AS task_id, 's' AS sender_member_id, 'u' AS recipient_member_id, 'note' AS message_type, 'body' AS body, '2026-09-03T00:00:00Z' AS created_at, NULL AS read_at, NULL AS acked_at"
    ).fetch_one(&pool).await.unwrap();
    let message: crate::backend::models::TeamMailboxMessage = row.into();
    assert_eq!(message.id, "m");
    assert_eq!(message.body, "body");
    assert_eq!(message.task_id, None);
    assert_eq!(message.acked_at, None);
}
```

## 步骤

- [ ] 跑旧 repo 测试建立 green；以上 typed row 单测初次因新类型缺失 red。
- [ ] Team 两处 SELECT 改 query_as，补重复 idempotency_key、ack/null、跨租户不可读的 repo 回归；写 SQL/排序/事务原样保留。
- [ ] Source 三处 load 改 query_as::<_,SourceRow>，用同一 TryFrom 保留 root/repo 路径 normalize、enum/json codec、enabled==1 和 nullable 字段。不同 SELECT 顺序仍由列名解码。
- [ ] 扩展 Source 回归：含/不含 repo_root、include/exclude JSON、legacy enum、另一租户同ID、非法 JSON 原错误路径不被默认空数组吞掉。
- [ ] 删除 map_sqlx_source_row 与仅为它使用的 SqliteRow/Row imports；Team 其他尚未迁移的 SQL若仍需 Row import 则保留，不机械清全文件。

```sh
cargo test -p assetiweave --lib typed_mailbox_row_preserves_nulls
cargo test -p assetiweave --lib team_repo
cargo test -p assetiweave --lib source_repo
cargo test -p assetiweave --lib backend::application::tests
cargo fmt --all -- --check
```

**完成：** 两个 repo 的目标映射由 FromRow 真实承担，通用重复读取删除，领域解码仍可定位测试；数据/schema 没有无关变化。
**API:** [SQLx FromRow](https://docs.rs/sqlx/latest/sqlx/trait.FromRow.html)。
