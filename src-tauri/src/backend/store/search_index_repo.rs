use crate::backend::{
    dto::SearchRetrievalMode,
    models::ConversationPart,
    runtime::{AppError, AppResult},
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

#[derive(Debug, Clone)]
pub(crate) struct ConversationSearchIndexDocumentRow {
    pub(crate) document_kind: String,
    pub(crate) record_kind: String,
    pub(crate) session_id: String,
    pub(crate) question_id: String,
    pub(crate) turn_id: String,
    pub(crate) part_id: String,
    pub(crate) block_id: String,
    pub(crate) card_kind: String,
    pub(crate) semantic_role: String,
    pub(crate) question_title: String,
    pub(crate) content: String,
    pub(crate) adapter_id: String,
    pub(crate) source_id: String,
    pub(crate) project_path: String,
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
#[cfg(test)]
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

pub(crate) async fn bump_conversation_search_source_revision_sqlx_tx(
    connection: &mut SqliteConnection,
    tenant_id: &str,
) -> AppResult<i64> {
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

#[derive(Debug, sqlx::FromRow)]
struct SearchIndexQuestionDocumentRow {
    session_id: String,
    question_id: String,
    turn_id: String,
    question_title: Option<String>,
    user_text: String,
    adapter_id: String,
    source_id: String,
    project_path: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct SearchIndexPartDocumentRow {
    session_id: String,
    question_id: String,
    turn_id: String,
    part_id: String,
    question_title: Option<String>,
    user_text: String,
    part_index: i64,
    role: String,
    kind: String,
    text: Option<String>,
    language: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    exit_code: Option<i64>,
    metadata_json: Option<String>,
    content_card_json: Option<String>,
    translated_text: Option<String>,
    source_execution_id: Option<String>,
    command_label: Option<String>,
    adapter_id: String,
    source_id: String,
    project_path: Option<String>,
    card_kinds_json: String,
}

pub(crate) async fn load_conversation_search_index_documents_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<ConversationSearchIndexDocumentRow>> {
    let mut documents = Vec::new();
    for tables in [SearchDocumentTables::session(), SearchDocumentTables::web()] {
        let question_sql = format!(
            r#"
            SELECT s.id AS session_id, q.id AS question_id, t.id AS turn_id, q.title AS question_title, t.user_text AS user_text,
                   s.adapter_id AS adapter_id, s.source_id AS source_id, {project_path} AS project_path
            FROM {sessions} s
            JOIN {questions} q ON q.tenant_id = s.tenant_id AND q.session_id = s.id
            JOIN {question_turns} qt ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
            JOIN {turns} t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
            WHERE s.tenant_id = ?1 AND s.missing = 0 AND t.missing = 0
            ORDER BY s.id, COALESCE((
                SELECT MIN(t_order.turn_index)
                FROM {question_turns} qt_order
                JOIN {turns} t_order
                  ON t_order.tenant_id = qt_order.tenant_id
                 AND t_order.id = qt_order.turn_id
                WHERE qt_order.tenant_id = q.tenant_id
                  AND qt_order.question_id = q.id
            ), 9223372036854775807), qt.turn_order
            "#,
            sessions = tables.sessions,
            questions = tables.questions,
            question_turns = tables.question_turns,
            turns = tables.turns,
            project_path = tables.project_path,
        );
        for row in sqlx::query_as::<_, SearchIndexQuestionDocumentRow>(AssertSqlSafe(question_sql))
            .bind(tenant_id)
            .fetch_all(pool)
            .await?
        {
            let turn_id = row.turn_id;
            let user_text = row.user_text;
            let question_title = search_question_title(row.question_title, &user_text);
            documents.push(ConversationSearchIndexDocumentRow {
                document_kind: "question".to_string(),
                record_kind: tables.record_kind.to_string(),
                session_id: row.session_id,
                question_id: row.question_id,
                turn_id: turn_id.clone(),
                part_id: String::new(),
                block_id: format!("{turn_id}-question"),
                card_kind: String::new(),
                semantic_role: String::new(),
                question_title,
                content: user_text,
                adapter_id: row.adapter_id,
                source_id: row.source_id,
                project_path: row.project_path.unwrap_or_default(),
            });
        }

        let part_sql = format!(
            r#"
            SELECT s.id AS session_id, q.id AS question_id, t.id AS turn_id, p.id AS part_id, q.title AS question_title, t.user_text AS user_text,
                   p.part_index AS part_index, p.role AS role, p.kind AS kind, p.text AS text, p.language AS language,
                   p.command AS command, p.cwd AS cwd, p.status AS status, p.exit_code AS exit_code,
                   p.metadata_json AS metadata_json, p.content_card_json AS content_card_json,
                   p.translated_text AS translated_text, p.source_execution_id AS source_execution_id,
                   p.command_label AS command_label, s.adapter_id AS adapter_id, s.source_id AS source_id,
                   {project_path} AS project_path, COALESCE(a.card_kinds_json, '[]') AS card_kinds_json
            FROM {sessions} s
            JOIN {questions} q ON q.tenant_id = s.tenant_id AND q.session_id = s.id
            JOIN {question_turns} qt ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
            JOIN {turns} t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
            JOIN {parts} p ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
            LEFT JOIN conversation_adapters a ON a.tenant_id = s.tenant_id AND a.id = s.adapter_id
            WHERE s.tenant_id = ?1 AND s.missing = 0 AND t.missing = 0
            ORDER BY s.id, COALESCE((
                SELECT MIN(t_order.turn_index)
                FROM {question_turns} qt_order
                JOIN {turns} t_order
                  ON t_order.tenant_id = qt_order.tenant_id
                 AND t_order.id = qt_order.turn_id
                WHERE qt_order.tenant_id = q.tenant_id
                  AND qt_order.question_id = q.id
            ), 9223372036854775807), qt.turn_order, p.part_index
            "#,
            sessions = tables.sessions,
            questions = tables.questions,
            question_turns = tables.question_turns,
            turns = tables.turns,
            parts = tables.parts,
            project_path = tables.project_path,
        );
        for row in sqlx::query_as::<_, SearchIndexPartDocumentRow>(AssertSqlSafe(part_sql))
            .bind(tenant_id)
            .fetch_all(pool)
            .await?
        {
            let part = map_search_part(&row)?;
            let card_kinds = serde_json::from_str::<
                Vec<crate::backend::models::ConversationCardKindDefinition>,
            >(&row.card_kinds_json)
            .unwrap_or_default();
            let adapter_id = row.adapter_id.clone();
            let cards =
                crate::backend::projection::conversation_cards::project_conversation_content_cards(
                    &part,
                    &adapter_id,
                    &card_kinds,
                )
                .map_err(AppError::external)?;
            let question_title = search_question_title(row.question_title.clone(), &row.user_text);
            for card in cards {
                let card_kind = card.kind;
                let semantic_role = card
                    .semantic_role
                    .or_else(|| {
                        card_kind
                            .rsplit_once('.')
                            .map(|(_, value)| value.to_string())
                    })
                    .unwrap_or_default();
                documents.push(ConversationSearchIndexDocumentRow {
                    document_kind: "card".to_string(),
                    record_kind: tables.record_kind.to_string(),
                    session_id: row.session_id.clone(),
                    question_id: row.question_id.clone(),
                    turn_id: row.turn_id.clone(),
                    part_id: part.id.clone(),
                    block_id: card.node_id,
                    card_kind,
                    semantic_role,
                    question_title: question_title.clone(),
                    content: card.body,
                    adapter_id: row.adapter_id.clone(),
                    source_id: row.source_id.clone(),
                    project_path: row.project_path.clone().unwrap_or_default(),
                });
            }
        }
    }
    Ok(documents)
}

fn map_search_part(row: &SearchIndexPartDocumentRow) -> AppResult<ConversationPart> {
    Ok(ConversationPart {
        id: row.part_id.clone(),
        turn_id: row.turn_id.clone(),
        part_index: row.part_index,
        role: super::codec::decode_enum(row.role.clone())?,
        kind: super::codec::decode_enum(row.kind.clone())?,
        text: row.text.clone(),
        language: row.language.clone(),
        command: row.command.clone(),
        cwd: row.cwd.clone(),
        status: row.status.clone(),
        exit_code: row.exit_code.map(|v| v as i32),
        metadata_json: row.metadata_json.clone(),
        command_label: row.command_label.clone(),
        source_execution_id: row.source_execution_id.clone(),
        content_card: row
            .content_card_json
            .clone()
            .map(super::codec::decode_json)
            .transpose()?,
        translated_text: row.translated_text.clone(),
    })
}

#[cfg(test)]
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

pub(crate) async fn complete_conversation_search_index_rebuild_with_offset_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    expected_revision: i64,
    generation: &str,
    document_count: i64,
    size_bytes: i64,
    consumer_offset: Option<(&str, i64)>,
) -> AppResult<bool> {
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
) -> AppResult<()> {
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
) -> AppResult<()> {
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

struct SearchDocumentTables {
    record_kind: &'static str,
    sessions: &'static str,
    questions: &'static str,
    question_turns: &'static str,
    turns: &'static str,
    parts: &'static str,
    project_path: &'static str,
}

impl SearchDocumentTables {
    fn session() -> Self {
        Self {
            record_kind: "session",
            sessions: "conversation_sessions",
            questions: "conversation_questions",
            question_turns: "conversation_question_turns",
            turns: "conversation_turns",
            parts: "conversation_parts",
            project_path: "COALESCE(s.project_path, '')",
        }
    }

    fn web() -> Self {
        Self {
            record_kind: "web",
            sessions: "web_record_sessions",
            questions: "web_record_questions",
            question_turns: "web_record_question_turns",
            turns: "web_record_turns",
            parts: "web_record_parts",
            project_path: "''",
        }
    }
}

fn search_question_title(title: Option<String>, question_text: &str) -> String {
    title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            question_text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Untitled question")
                .trim()
                .to_string()
        })
}

fn map_search_index_state(row: SearchIndexStateRow) -> AppResult<ConversationSearchIndexState> {
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

fn decode_search_index_health(value: &str) -> AppResult<ConversationSearchIndexHealth> {
    match value {
        "missing" => Ok(ConversationSearchIndexHealth::Missing),
        "ready" => Ok(ConversationSearchIndexHealth::Ready),
        "stale" => Ok(ConversationSearchIndexHealth::Stale),
        "failed" => Ok(ConversationSearchIndexHealth::Failed),
        "disabled" => Ok(ConversationSearchIndexHealth::Disabled),
        _ => Err(AppError::Validation(format!(
            "invalid conversation search index health: {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
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

        let initial =
            load_or_create_conversation_search_index_state_sqlx(database.pool(), TENANT_ID)
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
        let rebuilt =
            load_or_create_conversation_search_index_state_sqlx(database.pool(), TENANT_ID)
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
}
