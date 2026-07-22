// Removed once AppService exposes index status and rebuild operations.
#![allow(dead_code)]

use crate::backend::dto::{AppResult, SearchRetrievalMode};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

const CONVERSATION_SEARCH_SCHEMA_VERSION: i64 = 1;
const CONVERSATION_SEARCH_TOKENIZER_VERSION: &str = "tantivy-jieba-0.20.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConversationSearchIndexHealth {
    Missing,
    Ready,
    Stale,
    Failed,
    Disabled,
}

#[derive(Debug, Clone)]
pub(crate) struct ConversationSearchIndexState {
    pub(crate) tenant_id: String,
    pub(crate) index_instance_id: String,
    pub(crate) schema_version: i64,
    pub(crate) tokenizer_version: String,
    pub(crate) source_revision: i64,
    pub(crate) indexed_revision: Option<i64>,
    pub(crate) active_generation: Option<String>,
    pub(crate) health: ConversationSearchIndexHealth,
    pub(crate) document_count: i64,
    pub(crate) size_bytes: i64,
    pub(crate) last_built_at: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) lease_owner: Option<String>,
    pub(crate) lease_expires_at: Option<String>,
    pub(crate) updated_at: String,
}

impl ConversationSearchIndexState {
    pub(crate) fn supported_modes(&self) -> Vec<SearchRetrievalMode> {
        vec![SearchRetrievalMode::Lexical]
    }
}

pub(crate) async fn load_or_create_conversation_search_index_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<ConversationSearchIndexState> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO conversation_search_index_state (
            tenant_id, index_instance_id, schema_version, tokenizer_version, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(tenant_id) DO NOTHING
        "#,
    )
    .bind(tenant_id)
    .bind(Uuid::new_v4().to_string())
    .bind(CONVERSATION_SEARCH_SCHEMA_VERSION)
    .bind(CONVERSATION_SEARCH_TOKENIZER_VERSION)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;

    let row = sqlx::query(
        r#"
        SELECT tenant_id, index_instance_id, schema_version, tokenizer_version,
               source_revision, indexed_revision, active_generation, health,
               document_count, size_bytes, last_built_at, last_error,
               lease_owner, lease_expires_at, updated_at
        FROM conversation_search_index_state
        WHERE tenant_id = ?1
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|error| error.to_string())?;
    map_search_index_state(&row)
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
    .await
    .map_err(|error| error.to_string())?;
    Ok(revision)
}

pub(crate) async fn try_acquire_conversation_search_writer_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    owner: &str,
    now: &str,
    expires_at: &str,
) -> AppResult<bool> {
    load_or_create_conversation_search_index_state_sqlx(pool, tenant_id).await?;
    let result = sqlx::query(
        r#"
        UPDATE conversation_search_index_state
        SET lease_owner = ?1, lease_expires_at = ?2, updated_at = ?3
        WHERE tenant_id = ?4
          AND (
              lease_owner IS NULL
              OR lease_owner = ?1
              OR lease_expires_at IS NULL
              OR lease_expires_at <= ?3
          )
        "#,
    )
    .bind(owner)
    .bind(expires_at)
    .bind(now)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

fn map_search_index_state(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<ConversationSearchIndexState> {
    Ok(ConversationSearchIndexState {
        tenant_id: row.try_get(0).map_err(|error| error.to_string())?,
        index_instance_id: row.try_get(1).map_err(|error| error.to_string())?,
        schema_version: row.try_get(2).map_err(|error| error.to_string())?,
        tokenizer_version: row.try_get(3).map_err(|error| error.to_string())?,
        source_revision: row.try_get(4).map_err(|error| error.to_string())?,
        indexed_revision: row.try_get(5).map_err(|error| error.to_string())?,
        active_generation: row.try_get(6).map_err(|error| error.to_string())?,
        health: decode_search_index_health(
            &row.try_get::<String, _>(7)
                .map_err(|error| error.to_string())?,
        )?,
        document_count: row.try_get(8).map_err(|error| error.to_string())?,
        size_bytes: row.try_get(9).map_err(|error| error.to_string())?,
        last_built_at: row.try_get(10).map_err(|error| error.to_string())?,
        last_error: row.try_get(11).map_err(|error| error.to_string())?,
        lease_owner: row.try_get(12).map_err(|error| error.to_string())?,
        lease_expires_at: row.try_get(13).map_err(|error| error.to_string())?,
        updated_at: row.try_get(14).map_err(|error| error.to_string())?,
    })
}

fn decode_search_index_health(value: &str) -> AppResult<ConversationSearchIndexHealth> {
    match value {
        "missing" => Ok(ConversationSearchIndexHealth::Missing),
        "ready" => Ok(ConversationSearchIndexHealth::Ready),
        "stale" => Ok(ConversationSearchIndexHealth::Stale),
        "failed" => Ok(ConversationSearchIndexHealth::Failed),
        "disabled" => Ok(ConversationSearchIndexHealth::Disabled),
        _ => Err(format!("invalid conversation search index health: {value}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::store::Database;
    use std::path::PathBuf;
    use uuid::Uuid;

    const TENANT_ID: &str = "default";

    #[test]
    fn conversation_search_state_tracks_revision_and_writer_lease() {
        let db_path = temporary_database_path();
        let database = Database::open(&db_path).expect("open search state database");

        database
            .block_on(async {
                let initial =
                    load_or_create_conversation_search_index_state_sqlx(database.pool(), TENANT_ID)
                        .await?;
                assert_eq!(initial.health, ConversationSearchIndexHealth::Missing);
                assert_eq!(initial.source_revision, 0);
                assert_eq!(initial.indexed_revision, None);

                let revision =
                    bump_conversation_search_source_revision_sqlx(database.pool(), TENANT_ID)
                        .await?;
                assert_eq!(revision, 1);

                assert!(
                    try_acquire_conversation_search_writer_lease_sqlx(
                        database.pool(),
                        TENANT_ID,
                        "desktop",
                        "2026-07-22T10:00:00Z",
                        "2026-07-22T10:05:00Z",
                    )
                    .await?
                );
                assert!(
                    !try_acquire_conversation_search_writer_lease_sqlx(
                        database.pool(),
                        TENANT_ID,
                        "cli",
                        "2026-07-22T10:01:00Z",
                        "2026-07-22T10:06:00Z",
                    )
                    .await?
                );
                assert!(
                    try_acquire_conversation_search_writer_lease_sqlx(
                        database.pool(),
                        TENANT_ID,
                        "cli",
                        "2026-07-22T10:06:00Z",
                        "2026-07-22T10:11:00Z",
                    )
                    .await?
                );

                let state =
                    load_or_create_conversation_search_index_state_sqlx(database.pool(), TENANT_ID)
                        .await?;
                assert_eq!(state.source_revision, 1);
                assert_eq!(state.lease_owner.as_deref(), Some("cli"));
                crate::backend::dto::AppResult::Ok(())
            })
            .expect("track search state");

        drop(database);
        let _ = std::fs::remove_file(db_path);
    }

    fn temporary_database_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "assetiweave-conversation-search-state-{}.sqlite",
            Uuid::new_v4()
        ))
    }
}
