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

    let result = (|| {
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

        let root = conversation_search_index_root(db_path, tenant_id);
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        set_private_directory_permissions(&root)?;
        let generation = format!("generation-{}", Uuid::new_v4());
        let temporary_path = root.join(format!("{generation}.tmp"));
        let generation_path = root.join(&generation);
        let index = DiskConversationIndex::create(&temporary_path)?;
        set_private_directory_permissions(&temporary_path)?;
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

        let report = ConversationSearchIndexRebuildReport {
            generation,
            indexed_revision: state.source_revision,
            document_count,
            size_bytes,
            duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        };
        cleanup_old_generations(&root, &report.generation);
        Ok(report)
    })();

    if let Err(error) = &result {
        let pool = database.pool().clone();
        let tenant = tenant_id.to_string();
        let owner = owner.clone();
        let error = error.clone();
        let _ = database.block_on(async move {
            crate::backend::store::fail_conversation_search_index_rebuild_sqlx(
                &pool, &tenant, &owner, &error,
            )
            .await
        });
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn search_ready_conversation_index(
    database: &Database,
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
    let pool = database.pool().clone();
    let tenant = tenant_id.to_string();
    let state = database.block_on(async move {
        crate::backend::store::load_or_create_conversation_search_index_state_sqlx(&pool, &tenant)
            .await
    })?;
    if state.health.as_str() != "ready" || state.indexed_revision != Some(state.source_revision) {
        return Ok(None);
    }
    if !state.is_compatible() {
        mark_index_unusable(
            database,
            tenant_id,
            "conversation search index schema or tokenizer version is incompatible",
        );
        return Ok(None);
    }
    let Some(generation) = state.active_generation else {
        return Ok(None);
    };
    let path = conversation_search_index_root(db_path, tenant_id).join(generation);
    if !path.is_dir() {
        mark_index_unusable(
            database,
            tenant_id,
            "active conversation search index generation is missing",
        );
        return Ok(None);
    }
    let index = match DiskConversationIndex::open(&path) {
        Ok(index) => index,
        Err(error) => {
            mark_index_unusable(
                database,
                tenant_id,
                &format!("cannot open conversation search index: {error}"),
            );
            return Ok(None);
        }
    };
    index
        .search_cards(&ConversationCardQuery {
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
        })
        .map(Some)
}

fn conversation_search_index_root(db_path: &Path, tenant_id: &str) -> PathBuf {
    let database_root = db_path.parent().unwrap_or_else(|| Path::new("."));
    let database_hash = format!("{:x}", Sha256::digest(db_path.to_string_lossy().as_bytes()));
    let tenant_hash = format!("{:x}", Sha256::digest(tenant_id.as_bytes()));
    database_root
        .join("conversation-search-index")
        .join(&database_hash[..16])
        .join(&tenant_hash[..16])
}

fn mark_index_unusable(database: &Database, tenant_id: &str, error: &str) {
    let pool = database.pool().clone();
    let tenant = tenant_id.to_string();
    let error = error.to_string();
    let _ = database.block_on(async move {
        crate::backend::store::mark_conversation_search_index_unusable_sqlx(&pool, &tenant, &error)
            .await
    });
}

fn set_private_directory_permissions(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
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

fn cleanup_old_generations(root: &Path, active_generation: &str) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut generations = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("generation-") && !name.ends_with(".tmp")
        })
        .collect::<Vec<_>>();
    generations.sort_by_key(|entry| {
        std::cmp::Reverse(
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok(),
        )
    });
    let previous = generations
        .iter()
        .find(|entry| entry.file_name() != active_generation)
        .map(|entry| entry.path());
    for entry in generations {
        if entry.file_name() == active_generation || previous.as_ref() == Some(&entry.path()) {
            continue;
        }
        let _ = fs::remove_dir_all(entry.path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_roots_are_isolated_by_database_and_tenant() {
        let root = std::env::temp_dir().join("assetiweave-index-root-test");
        assert_ne!(
            conversation_search_index_root(&root.join("app.db"), "default"),
            conversation_search_index_root(&root.join("other.db"), "default")
        );
        assert_ne!(
            conversation_search_index_root(&root.join("app.db"), "default"),
            conversation_search_index_root(&root.join("app.db"), "tenant-b")
        );
    }

    #[cfg(unix)]
    #[test]
    fn search_index_directories_are_private_to_the_current_user() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("assetiweave-index-permissions-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create permissions fixture");
        set_private_directory_permissions(&root).expect("set private permissions");
        let mode = fs::metadata(&root)
            .expect("read permissions")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
        let _ = fs::remove_dir_all(root);
    }
}
