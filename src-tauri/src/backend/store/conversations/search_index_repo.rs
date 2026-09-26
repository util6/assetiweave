use crate::backend::{
    domain::SearchRetrievalMode,
    store::{StoreError, StoreResult},
};
use chrono::Utc;
use sqlx::{AssertSqlSafe, SqliteConnection, SqlitePool};
use uuid::Uuid;

const CONVERSATION_SEARCH_SCHEMA_VERSION: i64 = 4;
const CONVERSATION_SEARCH_TOKENIZER_VERSION: &str = "tantivy-jieba-0.20.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConversationSearchIndexHealth {
    Missing,
    Ready,
    Stale,
    Failed,
    Disabled,
}

impl ConversationSearchIndexHealth {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Ready => "ready",
            Self::Stale => "stale",
            Self::Failed => "failed",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConversationSearchIndexState {
    #[allow(dead_code)]
    pub(crate) tenant_id: String,
    #[allow(dead_code)]
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

    pub(crate) fn is_compatible(&self) -> bool {
        self.schema_version == CONVERSATION_SEARCH_SCHEMA_VERSION
            && self.tokenizer_version == CONVERSATION_SEARCH_TOKENIZER_VERSION
    }
}

pub(crate) async fn load_or_create_conversation_search_index_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<ConversationSearchIndexState> {
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
    .await?;

    let row = sqlx::query_as::<_, SearchIndexStateRow>(
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
    .await?;
    map_search_index_state(row)
}

#[allow(dead_code)]

pub(crate) async fn bump_conversation_search_source_revision_sqlx_tx(
    connection: &mut SqliteConnection,
    tenant_id: &str,
) -> StoreResult<i64> {
    let tenant_exists =
        sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM tenants WHERE id = ?1)")
            .bind(tenant_id)
            .fetch_one(&mut *connection)
            .await?;
    if tenant_exists == 0 {
        return Ok(0);
    }
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
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *connection)
    .await?;
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
    .fetch_one(connection)
    .await?;
    Ok(revision)
}

pub(crate) async fn try_acquire_conversation_search_writer_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    owner: &str,
    now: &str,
    expires_at: &str,
) -> StoreResult<bool> {
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
    .await?;
    Ok(result.rows_affected() == 1)
}

#[derive(Debug, sqlx::FromRow)]
struct SearchIndexStateRow {
    tenant_id: String,
    index_instance_id: String,
    schema_version: i64,
    tokenizer_version: String,
    source_revision: i64,
    indexed_revision: Option<i64>,
    active_generation: Option<String>,
    health: String,
    document_count: i64,
    size_bytes: i64,
    last_built_at: Option<String>,
    last_error: Option<String>,
    lease_owner: Option<String>,
    lease_expires_at: Option<String>,
    updated_at: String,
}

pub(crate) async fn complete_conversation_search_index_rebuild_with_offset_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    expected_revision: i64,
    generation: &str,
    document_count: i64,
    size_bytes: i64,
    consumer_offset: Option<(&str, i64)>,
) -> StoreResult<bool> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE conversation_search_index_state
        SET indexed_revision = ?1, active_generation = ?2, health = 'ready',
            document_count = ?3, size_bytes = ?4, last_built_at = ?5,
            last_error = NULL, lease_owner = NULL, lease_expires_at = NULL,
            schema_version = ?6, tokenizer_version = ?7, updated_at = ?5
        WHERE tenant_id = ?8 AND source_revision = ?1
        "#,
    )
    .bind(expected_revision)
    .bind(generation)
    .bind(document_count)
    .bind(size_bytes)
    .bind(&now)
    .bind(CONVERSATION_SEARCH_SCHEMA_VERSION)
    .bind(CONVERSATION_SEARCH_TOKENIZER_VERSION)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 1 {
        if let Some((consumer_id, last_seq)) = consumer_offset {
            sqlx::query(
                "INSERT INTO domain_event_consumer_offsets (consumer_id, tenant_id, last_seq, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (consumer_id, tenant_id) DO UPDATE SET last_seq = MAX(last_seq, excluded.last_seq), updated_at = excluded.updated_at",
            )
            .bind(consumer_id)
            .bind(tenant_id)
            .bind(last_seq)
            .bind(&now)
            .execute(&mut *tx)
            .await
            ?;
        }
    }
    tx.commit().await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn fail_conversation_search_index_rebuild_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    owner: &str,
    error: &str,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        UPDATE conversation_search_index_state
        SET health = 'failed', last_error = ?1, lease_owner = NULL,
            lease_expires_at = NULL, updated_at = ?2
        WHERE tenant_id = ?3 AND lease_owner = ?4
        "#,
    )
    .bind(error)
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .bind(owner)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn mark_conversation_search_index_unusable_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    error: &str,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        UPDATE conversation_search_index_state
        SET health = 'failed', last_error = ?1, updated_at = ?2
        WHERE tenant_id = ?3
        "#,
    )
    .bind(error)
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn map_search_index_state(row: SearchIndexStateRow) -> StoreResult<ConversationSearchIndexState> {
    Ok(ConversationSearchIndexState {
        tenant_id: row.tenant_id,
        index_instance_id: row.index_instance_id,
        schema_version: row.schema_version,
        tokenizer_version: row.tokenizer_version,
        source_revision: row.source_revision,
        indexed_revision: row.indexed_revision,
        active_generation: row.active_generation,
        health: decode_search_index_health(&row.health)?,
        document_count: row.document_count,
        size_bytes: row.size_bytes,
        last_built_at: row.last_built_at,
        last_error: row.last_error,
        lease_owner: row.lease_owner,
        lease_expires_at: row.lease_expires_at,
        updated_at: row.updated_at,
    })
}

fn decode_search_index_health(value: &str) -> StoreResult<ConversationSearchIndexHealth> {
    match value {
        "missing" => Ok(ConversationSearchIndexHealth::Missing),
        "ready" => Ok(ConversationSearchIndexHealth::Ready),
        "stale" => Ok(ConversationSearchIndexHealth::Stale),
        "failed" => Ok(ConversationSearchIndexHealth::Failed),
        "disabled" => Ok(ConversationSearchIndexHealth::Disabled),
        _ => Err(StoreError::Validation(format!(
            "invalid conversation search index health: {value}"
        ))),
    }
}

#[cfg(test)]
#[path = "search_index_repo_tests.rs"]
mod tests;
