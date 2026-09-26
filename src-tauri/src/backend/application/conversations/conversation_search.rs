use std::fs;
use std::path::Path;
use std::time::Instant;

use chrono::{Duration, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::conversations::{
    ConversationSearchIndexRebuildReport, ConversationSearchIndexStatus,
};
use crate::backend::domain::ConversationSearchMatches;
use crate::backend::infrastructure::search::conversation::{
    cleanup_old_generations, conversation_search_index_root, materialize_index_generation,
    search_generation_index, ConversationCardQuery, ConversationSearchDocument,
};

pub(crate) async fn rebuild_conversation_search_index(
    pool: &SqlitePool,
    db_path: &Path,
    tenant_id: &str,
) -> AppResult<ConversationSearchIndexRebuildReport> {
    rebuild_conversation_search_index_with_cancellation(pool, db_path, tenant_id, None).await
}

pub(crate) async fn rebuild_conversation_search_index_with_cancellation(
    pool: &SqlitePool,
    db_path: &Path,
    tenant_id: &str,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<ConversationSearchIndexRebuildReport> {
    rebuild_conversation_search_index_inner(pool, db_path, tenant_id, None, cancellation).await
}

pub(crate) async fn rebuild_conversation_search_index_with_offset(
    pool: &SqlitePool,
    db_path: &Path,
    tenant_id: &str,
    consumer_id: &str,
    last_seq: i64,
) -> AppResult<ConversationSearchIndexRebuildReport> {
    rebuild_conversation_search_index_inner(
        pool,
        db_path,
        tenant_id,
        Some((consumer_id, last_seq)),
        None,
    )
    .await
}

async fn rebuild_conversation_search_index_inner(
    pool: &SqlitePool,
    db_path: &Path,
    tenant_id: &str,
    consumer_offset: Option<(&str, i64)>,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<ConversationSearchIndexRebuildReport> {
    if cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
        return Err(AppError::Cancelled(
            "conversation search index rebuild cancelled".to_string(),
        ));
    }
    let started = Instant::now();
    let owner = format!("rebuild-{}", Uuid::new_v4());
    let now = Utc::now();
    let lease_expires_at = now + Duration::minutes(10);
    let acquired = crate::backend::store::try_acquire_conversation_search_writer_lease_sqlx(
        pool,
        tenant_id,
        &owner,
        &now.to_rfc3339(),
        &lease_expires_at.to_rfc3339(),
    )
    .await?;
    if !acquired {
        return Err(AppError::Conflict(
            "conversation search index is already being rebuilt".to_string(),
        ));
    }
    let root = conversation_search_index_root(db_path, tenant_id);
    let mut staged_path = None;

    let result: AppResult<ConversationSearchIndexRebuildReport> = async {
        let state = crate::backend::store::load_or_create_conversation_search_index_state_sqlx(
            pool, tenant_id,
        )
        .await?;
        let rows =
            crate::backend::store::load_conversation_search_index_documents_sqlx(pool, tenant_id)
                .await?;
        let documents = rows
            .into_iter()
            .map(|row| {
                ConversationSearchDocument::scoped_document(
                    &row.document_kind,
                    &row.record_kind,
                    &row.session_id,
                    &row.question_id,
                    &row.block_id,
                    &row.card_kind,
                    &row.semantic_role,
                    &row.question_title,
                    &row.content,
                    &row.adapter_id,
                    &row.source_id,
                    &row.project_path,
                    &row.turn_id,
                    &row.part_id,
                )
            })
            .collect::<Vec<_>>();

        let materialized = materialize_index_generation(&root, documents, cancellation).await?;
        staged_path = Some(materialized.generation_path.clone());

        let published =
            crate::backend::store::complete_conversation_search_index_rebuild_with_offset_sqlx(
                pool,
                tenant_id,
                state.source_revision,
                &materialized.generation,
                materialized.document_count,
                materialized.size_bytes,
                consumer_offset,
            )
            .await?;
        if !published {
            let _ = fs::remove_dir_all(&materialized.generation_path);
            return Err(AppError::Conflict(
                "conversation data changed during search index rebuild; rebuild again".to_string(),
            ));
        }

        let report = ConversationSearchIndexRebuildReport {
            generation: materialized.generation,
            indexed_revision: state.source_revision,
            document_count: materialized.document_count,
            size_bytes: materialized.size_bytes,
            duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        };
        cleanup_old_generations(&root, &report.generation);
        staged_path = None;
        Ok(report)
    }
    .await;

    if let Err(error) = &result {
        if let Some(path) = staged_path {
            let _ = fs::remove_dir_all(path);
        }
        let error_str = error.to_string();
        let _ = crate::backend::store::fail_conversation_search_index_rebuild_sqlx(
            pool, tenant_id, &owner, &error_str,
        )
        .await;
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn search_ready_conversation_index(
    pool: &SqlitePool,
    db_path: &Path,
    tenant_id: &str,
    query: String,
    record_kind: String,
    card_kinds: Vec<String>,
    semantic_roles: Vec<String>,
    include_questions: bool,
    include_cards: bool,
    adapter_id: Option<String>,
    source_id: Option<String>,
    project_path: Option<String>,
    limit: usize,
    offset: usize,
) -> AppResult<Option<ConversationSearchMatches>> {
    let state =
        crate::backend::store::load_or_create_conversation_search_index_state_sqlx(pool, tenant_id)
            .await?;
    if state.health.as_str() != "ready" || state.indexed_revision != Some(state.source_revision) {
        return Ok(None);
    }
    if !state.is_compatible() {
        mark_index_unusable(
            pool,
            tenant_id,
            "conversation search index schema or tokenizer version is incompatible",
        )
        .await;
        return Ok(None);
    }
    let Some(generation) = state.active_generation else {
        return Ok(None);
    };
    let path = conversation_search_index_root(db_path, tenant_id).join(generation);
    if !path.is_dir() {
        mark_index_unusable(
            pool,
            tenant_id,
            "active conversation search index generation is missing",
        )
        .await;
        return Ok(None);
    }
    let card_query = ConversationCardQuery {
        query,
        record_kind,
        card_kinds,
        semantic_roles,
        include_questions,
        include_cards,
        limit,
        offset,
        adapter_id,
        source_id,
        project_path,
    };
    match search_generation_index(&path, &card_query) {
        Ok(matches) => Ok(Some(matches)),
        Err(error) => {
            mark_index_unusable(
                pool,
                tenant_id,
                &format!("cannot open conversation search index: {error}"),
            )
            .await;
            Ok(None)
        }
    }
}

async fn mark_index_unusable(pool: &SqlitePool, tenant_id: &str, error: &str) {
    let _ =
        crate::backend::store::mark_conversation_search_index_unusable_sqlx(pool, tenant_id, error)
            .await;
}

impl AppService {
    pub(crate) async fn rebuild_conversation_search_index(
        &self,
    ) -> AppResult<ConversationSearchIndexRebuildReport> {
        self.rebuild_conversation_search_index_with_cancellation(None)
            .await
    }

    pub(crate) async fn rebuild_conversation_search_index_with_cancellation(
        &self,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
    ) -> AppResult<ConversationSearchIndexRebuildReport> {
        rebuild_conversation_search_index_with_cancellation(
            self.pool(),
            &self.db_path,
            self.tenant_id(),
            cancellation,
        )
        .await
    }

    pub(crate) async fn get_conversation_search_index_status(
        &self,
    ) -> AppResult<ConversationSearchIndexStatus> {
        let state = crate::backend::store::load_or_create_conversation_search_index_state_sqlx(
            self.pool(),
            self.tenant_id(),
        )
        .await?;

        Ok(status_from_state(state))
    }
}

fn status_from_state(
    state: crate::backend::store::ConversationSearchIndexState,
) -> ConversationSearchIndexStatus {
    let supported_modes = state.supported_modes();
    let is_rebuilding = state.lease_owner.is_some();
    let compatible = state.is_compatible();
    ConversationSearchIndexStatus {
        health: if compatible {
            state.health.as_str().to_string()
        } else {
            "failed".to_string()
        },
        schema_version: state.schema_version,
        tokenizer_version: state.tokenizer_version,
        source_revision: state.source_revision,
        indexed_revision: state.indexed_revision,
        active_generation: state.active_generation,
        document_count: state.document_count,
        size_bytes: state.size_bytes,
        last_built_at: state.last_built_at,
        last_error: state.last_error.or_else(|| {
            (!compatible).then(|| {
                "conversation search index schema or tokenizer version is incompatible".to_string()
            })
        }),
        lease_owner: state.lease_owner,
        lease_expires_at: state.lease_expires_at,
        is_rebuilding,
        updated_at: state.updated_at,
        supported_modes,
    }
}

#[cfg(test)]
#[path = "conversation_search_tests.rs"]
mod tests;
