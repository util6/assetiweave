use crate::backend::application::prelude::AppResult;
use crate::backend::{
    domain::ConversationSource, infrastructure::path_utils::normalize_path_for_storage,
};
use sqlx::SqlitePool;

pub async fn list_sources(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<ConversationSource>> {
    crate::backend::store::list_conversation_sources_sqlx(pool, tenant_id)
        .await?
        .into_iter()
        .map(normalize_source)
        .collect()
}

pub async fn load_source(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<Option<ConversationSource>> {
    crate::backend::store::load_conversation_source_sqlx(pool, tenant_id, source_id)
        .await?
        .map(normalize_source)
        .transpose()
}

pub async fn save_source(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
) -> AppResult<()> {
    let source = normalize_source(source.clone())?;
    Ok(crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &source).await?)
}

pub async fn disable_source(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<ConversationSource> {
    normalize_source(
        crate::backend::store::disable_conversation_source_sqlx(pool, tenant_id, source_id).await?,
    )
}

/// Rewrite persisted source locations into portable anchors.
///
/// Read paths already fold absolute host paths in memory, so a database that
/// still stores `/Users/<name>/...` looks healthy in the UI while the stored
/// truth stays non-portable. This pass repairs those rows once (startup) and is
/// a no-op afterwards; it never touches URI-style locations or sync watermarks.
pub async fn normalize_persisted_sources(pool: &SqlitePool, tenant_id: &str) -> AppResult<usize> {
    let mut repaired = 0;
    let sources = crate::backend::store::list_conversation_sources_sqlx(pool, tenant_id).await?;
    for mut source in sources {
        if source.location.contains("://") {
            continue;
        }
        let portable = normalize_path_for_storage(&source.location)?;
        if portable == source.location {
            continue;
        }
        tracing::info!(
            action = "conversation.source.location_normalized",
            tenant_id = %tenant_id,
            source_id = %source.id,
            from = %source.location,
            to = %portable,
            "repaired a non-portable conversation source location"
        );
        source.location = portable;
        crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &source).await?;
        repaired += 1;
    }
    Ok(repaired)
}

fn normalize_source(mut source: ConversationSource) -> AppResult<ConversationSource> {
    if !source.location.contains("://") {
        source.location = normalize_path_for_storage(&source.location)?;
    }
    Ok(source)
}

#[cfg(test)]
#[path = "conversation_sources_tests.rs"]
mod tests;
