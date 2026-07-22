use super::engine::{
    ConversationCardQuery, ConversationSearchDocument, ConversationSearchMatches,
    DiskConversationIndex,
};
use crate::backend::{
    dto::{AppResult, ConversationSearchIndexRebuildReport},
    store::Database,
};
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};
use uuid::Uuid;

pub(crate) fn rebuild_conversation_search_index(
    database: &Database,
    db_path: &Path,
    tenant_id: &str,
) -> AppResult<ConversationSearchIndexRebuildReport> {
    let started = Instant::now();
    let pool = database.pool().clone();
    let tenant_id_owned = tenant_id.to_string();
    let owner = format!("rebuild-{}", Uuid::new_v4());
    let now = Utc::now();
    let lease_expires_at = now + Duration::minutes(10);
    let state = database.block_on(async {
        let acquired = crate::backend::store::try_acquire_conversation_search_writer_lease_sqlx(
            &pool,
            &tenant_id_owned,
            &owner,
            &now.to_rfc3339(),
            &lease_expires_at.to_rfc3339(),
        )
        .await?;
        if !acquired {
            return Err("conversation search index is already being rebuilt".to_string());
        }
        crate::backend::store::load_or_create_conversation_search_index_state_sqlx(
            &pool,
            &tenant_id_owned,
        )
        .await
    })?;

    let pool = database.pool().clone();
    let tenant_for_load = tenant_id.to_string();
    let rows = database.block_on(async move {
        crate::backend::store::load_conversation_search_index_documents_sqlx(
            &pool,
            &tenant_for_load,
        )
        .await
    })?;
    let documents = rows
        .into_iter()
        .map(|row| {
            ConversationSearchDocument::scoped_card(
                &row.record_kind,
                &row.session_id,
                &row.question_id,
                &row.block_id,
                &row.card_type,
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

    let root = conversation_search_index_root(db_path, tenant_id);
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let generation = format!("generation-{}", Uuid::new_v4());
    let temporary_path = root.join(format!("{generation}.tmp"));
    let generation_path = root.join(&generation);
    let index = DiskConversationIndex::create(&temporary_path)?;
    index.replace_documents(&documents)?;
    drop(index);
    fs::rename(&temporary_path, &generation_path).map_err(|error| error.to_string())?;
    let size_bytes = directory_size(&generation_path)?;
    let document_count = i64::try_from(documents.len())
        .map_err(|_| "conversation search document count overflow".to_string())?;

    let pool = database.pool().clone();
    let tenant_for_publish = tenant_id.to_string();
    let generation_for_publish = generation.clone();
    let published = database.block_on(async move {
        crate::backend::store::complete_conversation_search_index_rebuild_sqlx(
            &pool,
            &tenant_for_publish,
            state.source_revision,
            &generation_for_publish,
            document_count,
            size_bytes,
        )
        .await
    })?;
    if !published {
        let _ = fs::remove_dir_all(&generation_path);
        return Err(
            "conversation data changed during search index rebuild; rebuild again".to_string(),
        );
    }

    Ok(ConversationSearchIndexRebuildReport {
        generation,
        indexed_revision: state.source_revision,
        document_count,
        size_bytes,
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn search_ready_conversation_index(
    database: &Database,
    db_path: &Path,
    tenant_id: &str,
    query: String,
    record_kind: String,
    card_types: Vec<String>,
    adapter_id: Option<String>,
    source_id: Option<String>,
    project_path: Option<String>,
    limit: usize,
    offset: usize,
) -> AppResult<Option<ConversationSearchMatches>> {
    let pool = database.pool().clone();
    let tenant = tenant_id.to_string();
    let state = database.block_on(async move {
        crate::backend::store::load_or_create_conversation_search_index_state_sqlx(&pool, &tenant)
            .await
    })?;
    if state.health.as_str() != "ready" || state.indexed_revision != Some(state.source_revision) {
        return Ok(None);
    }
    let Some(generation) = state.active_generation else {
        return Ok(None);
    };
    let path = conversation_search_index_root(db_path, tenant_id).join(generation);
    if !path.is_dir() {
        return Ok(None);
    }
    let index = match DiskConversationIndex::open(&path) {
        Ok(index) => index,
        Err(_) => return Ok(None),
    };
    index
        .search_cards(&ConversationCardQuery {
            query,
            record_kind,
            card_types,
            limit,
            offset,
            adapter_id,
            source_id,
            project_path,
        })
        .map(Some)
}

fn conversation_search_index_root(db_path: &Path, tenant_id: &str) -> PathBuf {
    let database_root = db_path.parent().unwrap_or_else(|| Path::new("."));
    let tenant_hash = format!("{:x}", Sha256::digest(tenant_id.as_bytes()));
    database_root
        .join("conversation-search-index")
        .join(&tenant_hash[..16])
}

fn directory_size(path: &Path) -> AppResult<i64> {
    let mut size = 0_u64;
    for entry in walkdir::WalkDir::new(path) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_file() {
            size = size.saturating_add(entry.metadata().map_err(|error| error.to_string())?.len());
        }
    }
    i64::try_from(size).map_err(|_| "conversation search index size overflow".to_string())
}
