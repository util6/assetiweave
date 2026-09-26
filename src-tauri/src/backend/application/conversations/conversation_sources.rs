use crate::backend::application::prelude::AppResult;
use crate::backend::{
    domain::ConversationSource, infrastructure::path_utils::normalize_path_for_storage,
};
use sqlx::SqlitePool;

pub(crate) async fn list_sources(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<ConversationSource>> {
    crate::backend::store::list_conversation_sources_sqlx(pool, tenant_id)
        .await?
        .into_iter()
        .map(normalize_source)
        .collect()
}

pub(crate) async fn load_source(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<Option<ConversationSource>> {
    crate::backend::store::load_conversation_source_sqlx(pool, tenant_id, source_id)
        .await?
        .map(normalize_source)
        .transpose()
}

pub(crate) async fn save_source(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
) -> AppResult<()> {
    let source = normalize_source(source.clone())?;
    Ok(crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &source).await?)
}

pub(crate) async fn disable_source(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<ConversationSource> {
    normalize_source(
        crate::backend::store::disable_conversation_source_sqlx(pool, tenant_id, source_id).await?,
    )
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
