use super::*;
use crate::backend::store::Database;
use std::path::PathBuf;
use uuid::Uuid;

const TENANT_ID: &str = "default";

#[tokio::test]
async fn conversation_search_state_tracks_revision_and_writer_lease() {
    let db_path = temporary_database_path();
    let database = Database::open_async(&db_path)
        .await
        .expect("open search state database");

    let initial = load_or_create_conversation_search_index_state_sqlx(database.pool(), TENANT_ID)
        .await
        .expect("load initial state");
    assert_eq!(initial.health, ConversationSearchIndexHealth::Missing);
    assert_eq!(initial.source_revision, 0);
    assert_eq!(initial.indexed_revision, None);
    assert!(initial.is_compatible());
    let mut previous_schema = initial.clone();
    previous_schema.schema_version = CONVERSATION_SEARCH_SCHEMA_VERSION - 1;
    assert!(!previous_schema.is_compatible());

    let revision = bump_conversation_search_source_revision_sqlx(database.pool(), TENANT_ID)
        .await
        .expect("bump revision");
    assert_eq!(revision, 1);

    assert!(try_acquire_conversation_search_writer_lease_sqlx(
        database.pool(),
        TENANT_ID,
        "desktop",
        "2026-07-22T10:00:00Z",
        "2026-07-22T10:05:00Z",
    )
    .await
    .expect("acquire lease"));
    assert!(!try_acquire_conversation_search_writer_lease_sqlx(
        database.pool(),
        TENANT_ID,
        "cli",
        "2026-07-22T10:01:00Z",
        "2026-07-22T10:06:00Z",
    )
    .await
    .expect("fail overlapping lease"));
    assert!(try_acquire_conversation_search_writer_lease_sqlx(
        database.pool(),
        TENANT_ID,
        "cli",
        "2026-07-22T10:06:00Z",
        "2026-07-22T10:11:00Z",
    )
    .await
    .expect("acquire subsequent lease"));

    let state = load_or_create_conversation_search_index_state_sqlx(database.pool(), TENANT_ID)
        .await
        .expect("load state");
    assert_eq!(state.source_revision, 1);
    assert_eq!(state.lease_owner.as_deref(), Some("cli"));

    sqlx::query(
        "UPDATE conversation_search_index_state SET schema_version = ?1 WHERE tenant_id = ?2",
    )
    .bind(CONVERSATION_SEARCH_SCHEMA_VERSION - 1)
    .bind(TENANT_ID)
    .execute(database.pool())
    .await
    .expect("update schema version");
    assert!(complete_conversation_search_index_rebuild_sqlx(
        database.pool(),
        TENANT_ID,
        revision,
        "generation-upgraded",
        12,
        4096,
    )
    .await
    .expect("complete rebuild"));
    let rebuilt = load_or_create_conversation_search_index_state_sqlx(database.pool(), TENANT_ID)
        .await
        .expect("load rebuilt state");
    assert!(rebuilt.is_compatible());
    assert_eq!(rebuilt.health, ConversationSearchIndexHealth::Ready);
    assert_eq!(
        rebuilt.active_generation.as_deref(),
        Some("generation-upgraded")
    );

    drop(database);
    let _ = std::fs::remove_file(db_path);
}

fn temporary_database_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "assetiweave-conversation-search-state-{}.sqlite",
        Uuid::new_v4()
    ))
}

pub(crate) async fn bump_conversation_search_source_revision_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<i64> {
    load_or_create_conversation_search_index_state_sqlx(pool, tenant_id).await?;
    let revision = sqlx::query_scalar::<_, i64>(
        r#"
        UPDATE conversation_search_index_state
        SET source_revision = source_revision + 1,
            health = CASE WHEN health = 'ready' THEN 'stale' ELSE health END,
            updated_at = ?1
        WHERE tenant_id = ?2
        RETURNING source_revision
        "#,
    )
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;
    Ok(revision)
}

pub(crate) async fn complete_conversation_search_index_rebuild_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    expected_revision: i64,
    generation: &str,
    document_count: i64,
    size_bytes: i64,
) -> AppResult<bool> {
    complete_conversation_search_index_rebuild_with_offset_sqlx(
        pool,
        tenant_id,
        expected_revision,
        generation,
        document_count,
        size_bytes,
        None,
    )
    .await
}
